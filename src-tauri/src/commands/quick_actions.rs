use serde::Serialize;
use serde_json::json;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use uuid::Uuid;

const REQUEST_MAGIC: &[u8] = b"CKQA1";
const REQUEST_PREFIX: &str = "convertkit-";
const REQUEST_EXTENSION: &str = "request";
const MAX_REQUEST_BYTES: u64 = 64 * 1024;
const MAX_REQUEST_PATHS: usize = 100;
const MAX_RECIPE_ID_BYTES: usize = 100;
const MAX_RECIPE_NAME_CHARS: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinderQuickActionRequest {
    pub recipe_id: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FinderQuickActionStatus {
    pub supported: bool,
    pub installed: bool,
    pub display_name: String,
}

fn validate_recipe_id(recipe_id: &str) -> Result<(), String> {
    if recipe_id.is_empty()
        || recipe_id.len() > MAX_RECIPE_ID_BYTES
        || !recipe_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err("The recipe identifier is invalid".into());
    }
    Ok(())
}

fn validate_recipe_name(recipe_name: &str) -> Result<String, String> {
    let name = recipe_name.trim();
    if name.is_empty() || name.chars().count() > MAX_RECIPE_NAME_CHARS {
        return Err("The recipe name is invalid".into());
    }
    if name.chars().any(char::is_control) {
        return Err("The recipe name contains unsupported characters".into());
    }
    Ok(name.to_string())
}

fn action_display_name(recipe_name: &str) -> String {
    format!("ConvertKit - {recipe_name}")
}

fn workflow_file_name(recipe_id: &str) -> String {
    format!("ConvertKit Recipe {recipe_id}.workflow")
}

fn services_directory() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("The user Library folder is unavailable")?;
    Ok(PathBuf::from(home).join("Library").join("Services"))
}

fn workflow_path_in(services_directory: &Path, recipe_id: &str) -> PathBuf {
    services_directory.join(workflow_file_name(recipe_id))
}

fn shell_script(recipe_id: &str) -> String {
    format!(
        r#"set -eu
umask 077
request_dir="${{TMPDIR:-/tmp}}/com.dropforge.convertkit/quick-actions"
/bin/mkdir -p "$request_dir"
/bin/chmod 700 "$request_dir"
request_file=$(/usr/bin/mktemp "$request_dir/convertkit-XXXXXXXX.request")
{{
  /usr/bin/printf 'CKQA1\0%s\0' '{recipe_id}'
  /usr/bin/printf '%s\0' "$@"
}} > "$request_file"
/usr/bin/open -b com.dropforge.convertkit "$request_file"
"#
    )
}

fn argument_definition(name: &str, value: serde_json::Value, uuid: &str) -> serde_json::Value {
    json!({
        "default value": value,
        "name": name,
        "required": "0",
        "type": "0",
        "uuid": uuid,
    })
}

fn write_workflow(
    services_directory: &Path,
    recipe_id: &str,
    recipe_name: &str,
) -> Result<PathBuf, String> {
    write_workflow_with_script(
        services_directory,
        recipe_id,
        recipe_name,
        &shell_script(recipe_id),
    )
}

fn write_workflow_with_script(
    services_directory: &Path,
    recipe_id: &str,
    recipe_name: &str,
    script: &str,
) -> Result<PathBuf, String> {
    fs::create_dir_all(services_directory)
        .map_err(|error| format!("Could not open the Finder services folder: {error}"))?;

    let target = workflow_path_in(services_directory, recipe_id);
    if target.exists() {
        return Ok(target);
    }

    let temporary = services_directory.join(format!(".convertkit-{}.workflow", Uuid::new_v4()));
    let contents = temporary.join("Contents");
    fs::create_dir_all(&contents)
        .map_err(|error| format!("Could not prepare the Finder action: {error}"))?;

    let result = (|| {
        let display_name = action_display_name(recipe_name);
        let action_version = plist::Value::from_file(
            "/System/Library/Automator/Run Shell Script.action/Contents/Info.plist",
        )
        .ok()
        .and_then(|value| {
            value
                .as_dictionary()?
                .get("CFBundleVersion")?
                .as_string()
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "2.0.3".into());

        let action = json!({
            "AMAccepts": {
                "Container": "List",
                "Optional": true,
                "Types": ["com.apple.cocoa.string"],
            },
            "AMActionVersion": action_version,
            "AMApplication": ["Automator"],
            "AMProvides": {
                "Container": "List",
                "Types": ["com.apple.cocoa.string"],
            },
            "ActionBundlePath": "/System/Library/Automator/Run Shell Script.action",
            "ActionName": "Run Shell Script",
            "ActionParameters": {
                "COMMAND_STRING": script,
                "CheckedForUserDefaultShell": true,
                "inputMethod": 1,
                "shell": "/bin/zsh",
                "source": script,
            },
            "BundleIdentifier": "com.apple.RunShellScript",
            "CFBundleVersion": action_version,
            "CanShowSelectedItemsWhenRun": false,
            "CanShowWhenRun": true,
            "Category": ["AMCategoryUtilities"],
            "Class Name": "RunShellScriptAction",
            "InputUUID": Uuid::new_v4().to_string().to_uppercase(),
            "Keywords": ["Shell", "Script", "Command", "Run", "Unix"],
            "OutputUUID": Uuid::new_v4().to_string().to_uppercase(),
            "UUID": Uuid::new_v4().to_string().to_uppercase(),
            "UnlocalizedApplications": ["Automator"],
            "arguments": {
                "0": argument_definition("inputMethod", json!(1), "0"),
                "1": argument_definition("CheckedForUserDefaultShell", json!(true), "1"),
                "2": argument_definition("source", json!(script), "2"),
                "3": argument_definition("COMMAND_STRING", json!(script), "3"),
                "4": argument_definition("shell", json!("/bin/zsh"), "4"),
            },
            "nibPath": "/System/Library/Automator/Run Shell Script.action/Contents/Resources/Base.lproj/main.nib",
        });

        let document = json!({
            "AMDocumentVersion": "2",
            "actions": [{ "action": action }],
            "connectors": {},
            "workflowMetaData": {
                "serviceApplicationBundleID": "com.apple.finder",
                "serviceApplicationPath": "/System/Library/CoreServices/Finder.app",
                "serviceInputTypeIdentifier": "com.apple.Automator.fileSystemObject",
                "serviceOutputTypeIdentifier": "com.apple.Automator.nothing",
                "workflowTypeIdentifier": "com.apple.Automator.servicesMenu",
            },
        });
        plist::to_file_xml(contents.join("document.wflow"), &document)
            .map_err(|error| format!("Could not write the Finder action workflow: {error}"))?;

        let info = json!({
            "CFBundleDevelopmentRegion": "en_US",
            "CFBundleIdentifier": format!("com.dropforge.convertkit.quick-action.{recipe_id}"),
            "CFBundleName": display_name,
            "CFBundleShortVersionString": "1.0",
            "NSServices": [{
                "NSMenuItem": { "default": display_name },
                "NSMessage": "runWorkflowAsService",
                "NSRequiredContext": { "NSApplicationIdentifier": "com.apple.finder" },
                "NSSendFileTypes": ["public.item"],
            }],
        });
        plist::to_file_xml(contents.join("Info.plist"), &info)
            .map_err(|error| format!("Could not register the Finder action: {error}"))?;

        fs::rename(&temporary, &target)
            .map_err(|error| format!("Could not install the Finder action: {error}"))?;
        Ok(target.clone())
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

#[tauri::command]
pub fn get_finder_quick_action_status(
    recipe_id: String,
    recipe_name: String,
) -> Result<FinderQuickActionStatus, String> {
    validate_recipe_id(&recipe_id)?;
    let recipe_name = validate_recipe_name(&recipe_name)?;

    #[cfg(target_os = "macos")]
    {
        let path = workflow_path_in(&services_directory()?, &recipe_id);
        Ok(FinderQuickActionStatus {
            supported: true,
            installed: path.is_dir(),
            display_name: action_display_name(&recipe_name),
        })
    }

    #[cfg(not(target_os = "macos"))]
    Ok(FinderQuickActionStatus {
        supported: false,
        installed: false,
        display_name: action_display_name(&recipe_name),
    })
}

#[tauri::command]
pub fn install_finder_quick_action(
    recipe_id: String,
    recipe_name: String,
) -> Result<FinderQuickActionStatus, String> {
    validate_recipe_id(&recipe_id)?;
    let recipe_name = validate_recipe_name(&recipe_name)?;

    #[cfg(target_os = "macos")]
    {
        write_workflow(&services_directory()?, &recipe_id, &recipe_name)?;
        Ok(FinderQuickActionStatus {
            supported: true,
            installed: true,
            display_name: action_display_name(&recipe_name),
        })
    }

    #[cfg(not(target_os = "macos"))]
    Err("Finder Quick Actions are only available on macOS".into())
}

#[tauri::command]
pub fn remove_finder_quick_action(recipe_id: String) -> Result<(), String> {
    validate_recipe_id(&recipe_id)?;

    #[cfg(target_os = "macos")]
    {
        let path = workflow_path_in(&services_directory()?, &recipe_id);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_symlink() => fs::remove_file(path)
                .map_err(|error| format!("Could not remove the Finder action: {error}")),
            Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(path)
                .map_err(|error| format!("Could not remove the Finder action: {error}")),
            Ok(_) => Err("The Finder action path is not a workflow".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(format!("Could not inspect the Finder action: {error}")),
        }
    }

    #[cfg(not(target_os = "macos"))]
    Ok(())
}

pub(crate) fn consume_finder_quick_action_request(
    path: &Path,
) -> Result<Option<FinderQuickActionRequest>, String> {
    let file_name = path.file_name().and_then(|name| name.to_str());
    if !file_name.is_some_and(|name| name.starts_with(REQUEST_PREFIX))
        || path.extension().and_then(|extension| extension.to_str()) != Some(REQUEST_EXTENSION)
    {
        return Ok(None);
    }

    let metadata = fs::symlink_metadata(path)
        .map_err(|error| format!("Could not inspect the Finder action request: {error}"))?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_REQUEST_BYTES {
        return Ok(None);
    }

    let bytes = fs::read(path)
        .map_err(|error| format!("Could not read the Finder action request: {error}"))?;
    let mut fields = bytes.split(|byte| *byte == 0);
    if fields.next() != Some(REQUEST_MAGIC) {
        return Ok(None);
    }

    let parsed = (|| {
        let recipe_id =
            std::str::from_utf8(fields.next().ok_or("The recipe identifier is missing")?)
                .map_err(|_| "The recipe identifier is invalid")?
                .to_string();
        validate_recipe_id(&recipe_id)?;

        let mut paths = Vec::new();
        for field in fields.filter(|field| !field.is_empty()) {
            if paths.len() == MAX_REQUEST_PATHS {
                return Err("The Finder action selected too many files".into());
            }
            let path = std::str::from_utf8(field)
                .map_err(|_| "A selected file path is invalid")?
                .to_string();
            paths.push(path);
        }
        if paths.is_empty() {
            return Err("The Finder action did not include any files".into());
        }
        Ok(FinderQuickActionRequest { recipe_id, paths })
    })();

    let _ = fs::remove_file(path);
    parsed.map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_valid_finder_service_workflow() {
        let directory = tempfile::tempdir().unwrap();
        let path = write_workflow(directory.path(), "recipe-1", "Web images").unwrap();
        let document = plist::Value::from_file(path.join("Contents/document.wflow")).unwrap();
        let info = plist::Value::from_file(path.join("Contents/Info.plist")).unwrap();

        assert_eq!(
            document
                .as_dictionary()
                .unwrap()
                .get("workflowMetaData")
                .unwrap()
                .as_dictionary()
                .unwrap()
                .get("workflowTypeIdentifier")
                .unwrap()
                .as_string(),
            Some("com.apple.Automator.servicesMenu")
        );
        assert_eq!(
            info.as_dictionary()
                .unwrap()
                .get("CFBundleName")
                .unwrap()
                .as_string(),
            Some("ConvertKit - Web images")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn generated_workflow_runs_through_automator() {
        let directory = tempfile::tempdir().unwrap();
        let path = write_workflow_with_script(
            directory.path(),
            "recipe-1",
            "Web images",
            "/usr/bin/printf 'workflow-ok'",
        )
        .unwrap();
        let output = std::process::Command::new("/usr/bin/automator")
            .arg(&path)
            .output()
            .unwrap();

        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("workflow-ok"));
    }

    #[test]
    fn consumes_bounded_nul_delimited_requests() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("convertkit-test.request");
        fs::write(&path, b"CKQA1\0recipe-1\0/tmp/a file.png\0/tmp/b.png\0").unwrap();

        let request = consume_finder_quick_action_request(&path).unwrap().unwrap();
        assert_eq!(request.recipe_id, "recipe-1");
        assert_eq!(request.paths, ["/tmp/a file.png", "/tmp/b.png"]);
        assert!(!path.exists());
    }

    #[test]
    fn leaves_regular_files_untouched() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("photo.png");
        fs::write(&path, b"not a request").unwrap();

        assert!(consume_finder_quick_action_request(&path)
            .unwrap()
            .is_none());
        assert!(path.exists());
    }
}
