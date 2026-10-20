//! Lossless PDF page operations on lopdf: copy pages, with every object they
//! reach, from one or more documents into a new one. This is what Poppler's
//! pdfseparate and pdfunite do, so split, extract and combine need no
//! external tool.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;

use lopdf::{Dictionary, Document, Object, ObjectId};
use tokio_util::sync::CancellationToken;

use crate::error::ConversionError;

/// Page attributes a page may inherit from its ancestors in the page tree
/// (PDF 32000-1:2008, table 30). They are written onto each copied page
/// because the copy gets a new, flat parent.
const INHERITED_KEYS: [&[u8]; 4] = [b"Resources", b"MediaBox", b"CropBox", b"Rotate"];
const MAX_PAGE_TREE_DEPTH: usize = 64;

fn pdf_error(message: &str, detail: impl std::fmt::Display) -> ConversionError {
    ConversionError::ProcessFailed {
        message: message.into(),
        stderr: detail.to_string(),
        exit_code: None,
    }
}

/// Load a PDF, opening documents protected only by an owner password the
/// way Poppler does. Documents that need a user password are rejected.
pub fn load(path: &Path) -> Result<Document, ConversionError> {
    let mut document = Document::load(path)
        .map_err(|error| pdf_error("This PDF could not be read", error))?;
    if document.is_encrypted() {
        document.decrypt("").map_err(|_| ConversionError::UnsupportedConversion {
            input: "This PDF is password-protected".into(),
            output: "an unlocked PDF".into(),
        })?;
    }
    if document.get_pages().is_empty() {
        return Err(pdf_error("This PDF has no pages", ""));
    }
    Ok(document)
}

/// A new document holding `pages` (1-based, in the given order) of `source`.
pub fn extract(source: &Document, pages: &[u32]) -> Result<Document, ConversionError> {
    let mut builder = Builder::new(source.version.clone());
    builder.append(source, pages)?;
    Ok(builder.finish())
}

/// Every page of every source, in order, as one document.
pub fn merge(
    sources: &[Document],
    cancel_token: &CancellationToken,
) -> Result<Document, ConversionError> {
    let version = sources
        .iter()
        .map(|source| source.version.clone())
        .max()
        .unwrap_or_else(|| "1.7".into());
    let mut builder = Builder::new(version);
    for source in sources {
        if cancel_token.is_cancelled() {
            return Err(ConversionError::Cancelled);
        }
        let pages = source.get_pages().keys().copied().collect::<Vec<_>>();
        builder.append(source, &pages)?;
    }
    Ok(builder.finish())
}

pub fn save(document: &mut Document, path: &Path) -> Result<u64, ConversionError> {
    document
        .save(path)
        .map_err(|error| pdf_error("The PDF could not be written", error))?;
    crate::engines::verification::file_with_signature(path, b"%PDF-")
}

struct Builder {
    target: Document,
    pages_id: ObjectId,
    kids: Vec<Object>,
}

impl Builder {
    fn new(version: String) -> Self {
        let mut target = Document::with_version(version);
        let pages_id = target.new_object_id();
        Self {
            target,
            pages_id,
            kids: Vec::new(),
        }
    }

    fn append(&mut self, source: &Document, pages: &[u32]) -> Result<(), ConversionError> {
        let numbered = source.get_pages();
        let selected = pages
            .iter()
            .map(|page| {
                numbered.get(page).copied().ok_or_else(|| ConversionError::UnsupportedConversion {
                    input: format!("Page {page} does not exist"),
                    output: format!("a page between 1 and {}", numbered.len()),
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut copier = Copier {
            source,
            target: &mut self.target,
            map: HashMap::new(),
            queue: VecDeque::new(),
            all_pages: numbered.values().copied().collect(),
        };
        // Selected pages get their ids first, so links between them survive
        // and a page selected twice is copied twice.
        let mut new_pages = Vec::with_capacity(selected.len());
        for &page_id in &selected {
            let new_id = copier.target.new_object_id();
            copier.map.entry(page_id).or_insert(new_id);
            new_pages.push((page_id, new_id));
        }
        for (page_id, new_id) in new_pages {
            let mut page = inherited_page(source, page_id)?;
            page.remove(b"Parent");
            let mut copied = copier.copy_direct(&Object::Dictionary(page));
            if let Object::Dictionary(dictionary) = &mut copied {
                dictionary.set("Parent", Object::Reference(self.pages_id));
            }
            copier.target.objects.insert(new_id, copied);
            self.kids.push(Object::Reference(new_id));
        }
        copier.drain();
        Ok(())
    }

    fn finish(mut self) -> Document {
        let count = self.kids.len() as i64;
        let mut pages = Dictionary::new();
        pages.set("Type", Object::Name(b"Pages".to_vec()));
        pages.set("Kids", Object::Array(self.kids));
        pages.set("Count", Object::Integer(count));
        self.target
            .objects
            .insert(self.pages_id, Object::Dictionary(pages));
        let mut catalog = Dictionary::new();
        catalog.set("Type", Object::Name(b"Catalog".to_vec()));
        catalog.set("Pages", Object::Reference(self.pages_id));
        let catalog_id = self.target.add_object(Object::Dictionary(catalog));
        self.target.trailer.set("Root", Object::Reference(catalog_id));
        self.target
    }
}

/// The page dictionary with inherited attributes copied down from its
/// ancestors.
fn inherited_page(source: &Document, page_id: ObjectId) -> Result<Dictionary, ConversionError> {
    let mut page = source
        .get_dictionary(page_id)
        .map_err(|error| pdf_error("A PDF page could not be read", error))?
        .clone();
    let mut parent = page.get(b"Parent").and_then(Object::as_reference).ok();
    let mut depth = 0;
    while let Some(parent_id) = parent {
        depth += 1;
        if depth > MAX_PAGE_TREE_DEPTH {
            return Err(pdf_error("The PDF page tree is too deep", ""));
        }
        let Ok(node) = source.get_dictionary(parent_id) else {
            break;
        };
        for key in INHERITED_KEYS {
            if !page.has(key) {
                if let Ok(value) = node.get(key) {
                    page.set(key, value.clone());
                }
            }
        }
        parent = node.get(b"Parent").and_then(Object::as_reference).ok();
    }
    Ok(page)
}

struct Copier<'a> {
    source: &'a Document,
    target: &'a mut Document,
    map: HashMap<ObjectId, ObjectId>,
    queue: VecDeque<ObjectId>,
    all_pages: HashSet<ObjectId>,
}

impl Copier<'_> {
    /// Copy a direct object, renumbering every reference into the target.
    /// Referenced objects are queued rather than recursed into, so long
    /// chains cannot exhaust the stack.
    fn copy_direct(&mut self, object: &Object) -> Object {
        match object {
            Object::Reference(id) => self.reference(*id),
            Object::Array(items) => {
                Object::Array(items.iter().map(|item| self.copy_direct(item)).collect())
            }
            Object::Dictionary(dictionary) => Object::Dictionary(self.copy_dictionary(dictionary)),
            Object::Stream(stream) => {
                let mut copied = stream.clone();
                copied.dict = self.copy_dictionary(&stream.dict);
                Object::Stream(copied)
            }
            other => other.clone(),
        }
    }

    fn copy_dictionary(&mut self, dictionary: &Dictionary) -> Dictionary {
        let mut copied = Dictionary::new();
        for (key, value) in dictionary.iter() {
            copied.set(key.clone(), self.copy_direct(value));
        }
        copied
    }

    fn reference(&mut self, id: ObjectId) -> Object {
        if let Some(new_id) = self.map.get(&id) {
            return Object::Reference(*new_id);
        }
        // A link to a page that is not being copied would drag that whole
        // page along; it becomes null, as in pdfseparate.
        if self.all_pages.contains(&id) {
            return Object::Null;
        }
        let new_id = self.target.new_object_id();
        self.map.insert(id, new_id);
        self.queue.push_back(id);
        Object::Reference(new_id)
    }

    fn drain(&mut self) {
        while let Some(id) = self.queue.pop_front() {
            let new_id = self.map[&id];
            let object = self
                .source
                .get_object(id)
                .map(|object| self.copy_direct(&object.clone()))
                .unwrap_or(Object::Null);
            self.target.objects.insert(new_id, object);
        }
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use lopdf::{dictionary, Document, Object, Stream};
    use std::path::Path;

    /// A PDF whose pages inherit MediaBox, Resources and Rotate from an
    /// intermediate page-tree node, with a link from page 1 to page 3.
    pub fn nested_pdf(path: &Path, labels: &[&str]) {
        let mut document = Document::with_version("1.7");
        let root_pages = document.new_object_id();
        let middle = document.new_object_id();
        let font = document.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let mut kids = Vec::new();
        for label in labels {
            let content = format!("BT /F1 24 Tf 72 700 Td ({label}) Tj ET");
            let stream = document.add_object(Stream::new(dictionary! {}, content.into_bytes()));
            kids.push(document.add_object(dictionary! {
                "Type" => "Page",
                "Parent" => middle,
                "Contents" => stream,
            }));
        }
        if kids.len() >= 3 {
            let link = document.add_object(dictionary! {
                "Type" => "Annot", "Subtype" => "Link",
                "Rect" => vec![0.into(), 0.into(), 10.into(), 10.into()],
                "Dest" => vec![kids[2].into(), "Fit".into()],
            });
            let first = document.get_dictionary_mut(kids[0]).expect("first page");
            first.set("Annots", vec![Object::Reference(link)]);
            first.set("Rotate", 90);
        }
        document.objects.insert(
            middle,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Parent" => root_pages,
                "Kids" => kids.iter().map(|id| Object::Reference(*id)).collect::<Vec<_>>(),
                "Count" => kids.len() as i64,
                "MediaBox" => vec![0.into(), 0.into(), 300.into(), 400.into()],
                "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
            }),
        );
        document.objects.insert(
            root_pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![Object::Reference(middle)],
                "Count" => kids.len() as i64,
            }),
        );
        let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => root_pages });
        document.trailer.set("Root", catalog);
        document.save(path).expect("save fixture");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page_text(document: &Document, page: u32) -> String {
        String::from_utf8_lossy(&document.extract_text(&[page]).unwrap_or_default().into_bytes())
            .trim()
            .to_owned()
    }

    #[test]
    fn extracts_pages_with_inherited_attributes_and_cut_links() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("source.pdf");
        fixtures::nested_pdf(&input, &["One", "Two", "Three"]);
        let source = load(&input).expect("load");

        let mut first = extract(&source, &[1]).expect("extract");
        let output = directory.path().join("first.pdf");
        save(&mut first, &output).expect("save");
        let reloaded = load(&output).expect("reload");
        assert_eq!(reloaded.get_pages().len(), 1);
        let page_id = reloaded.get_pages()[&1];
        let page = reloaded.get_dictionary(page_id).expect("page");
        assert_eq!(page.get(b"Rotate").and_then(Object::as_i64).ok(), Some(90));
        assert!(page.get(b"MediaBox").is_ok(), "inherited MediaBox is materialised");
        assert!(page.get(b"Resources").is_ok(), "inherited Resources are materialised");
        assert!(page_text(&reloaded, 1).contains("One"));
        // The link to page 3 no longer pulls page 3 in.
        let link_dest = reloaded
            .objects
            .values()
            .filter_map(|object| object.as_dict().ok())
            .find(|dictionary| dictionary.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Link"))
            .and_then(|link| link.get(b"Dest").ok().cloned());
        assert_eq!(link_dest, Some(Object::Array(vec![Object::Null, "Fit".into()])));
        assert_eq!(
            reloaded
                .objects
                .values()
                .filter(|object| object
                    .as_dict()
                    .ok()
                    .and_then(|dictionary| dictionary.get(b"Type").and_then(Object::as_name).ok())
                    == Some(b"Page"))
                .count(),
            1
        );

        let reordered = extract(&source, &[3, 1]).expect("reorder");
        assert_eq!(page_text(&reordered, 1), "Three");
        assert_eq!(page_text(&reordered, 2), "One");
        assert!(extract(&source, &[4]).is_err());
    }

    #[test]
    fn merges_documents_in_order() {
        let directory = tempfile::tempdir().expect("tempdir");
        let first = directory.path().join("a.pdf");
        let second = directory.path().join("b.pdf");
        fixtures::nested_pdf(&first, &["A1", "A2"]);
        fixtures::nested_pdf(&second, &["B1"]);
        let sources = vec![load(&first).expect("a"), load(&second).expect("b")];
        let mut merged = merge(&sources, &CancellationToken::new()).expect("merge");
        let output = directory.path().join("merged.pdf");
        save(&mut merged, &output).expect("save");
        let reloaded = load(&output).expect("reload");
        assert_eq!(reloaded.get_pages().len(), 3);
        assert_eq!(
            (1..=3).map(|page| page_text(&reloaded, page)).collect::<Vec<_>>(),
            ["A1", "A2", "B1"]
        );

        let cancelled = CancellationToken::new();
        cancelled.cancel();
        assert!(matches!(merge(&sources, &cancelled), Err(ConversionError::Cancelled)));
    }
}
