import assert from "node:assert/strict";
import test from "node:test";

import {
  ARCHIVE_FORMATS,
  archiveEntryRequest,
  archiveFormatLabel,
  archivePathLabel,
  gzipCreationInputError,
  isSupportedArchivePath,
} from "../src/lib/archive.ts";
import { isPathSupportedForOperation } from "../src/lib/operations.ts";

test("labels every archive creation format", () => {
  assert.deepEqual(ARCHIVE_FORMATS, ["zip", "tar", "tarGz", "sevenZ", "gzip"]);
  assert.deepEqual(ARCHIVE_FORMATS.map(archiveFormatLabel), ["ZIP", "TAR", "TAR.GZ", "7Z", "GZIP"]);
});

test("GZIP creation accepts one direct file and explains rejected queues", () => {
  assert.equal(gzipCreationInputError([{ relativePath: null }]), null);
  assert.equal(gzipCreationInputError([]), "GZIP compresses exactly one file at a time.");
  assert.equal(
    gzipCreationInputError([{ relativePath: null }, { relativePath: null }]),
    "GZIP compresses exactly one file at a time.",
  );
  assert.equal(
    gzipCreationInputError([{ relativePath: "folder/report.csv" }]),
    "GZIP cannot compress a folder. Choose one regular file directly.",
  );
});

test("archive requests retain whether a file came from a folder", () => {
  assert.deepEqual(
    archiveEntryRequest({
      path: "/tmp/source/report.csv",
      name: "report.csv",
      relativePath: "source\\report.csv",
    }),
    {
      inputPath: "/tmp/source/report.csv",
      archivePath: "source/report.csv",
      folderDerived: true,
    },
  );
  assert.deepEqual(
    archiveEntryRequest({ path: "/tmp/report.csv", name: "report.csv", relativePath: null }),
    {
      inputPath: "/tmp/report.csv",
      archivePath: "report.csv",
      folderDerived: false,
    },
  );
});

test("accepts standalone GZIP while keeping it distinct from TAR.GZ and TGZ", () => {
  for (const path of [
    "bundle.zip",
    "bundle.7z",
    "bundle.7Z",
    "bundle.TAR",
    "bundle.tar.gz",
    "bundle.TGZ",
    "document.gz",
    "document.GZ",
  ]) {
    assert.equal(isSupportedArchivePath(path), true, path);
    assert.equal(isPathSupportedForOperation(path, "extractArchive"), true, path);
  }
  assert.equal(archivePathLabel("bundle.7z"), "7Z");
  assert.equal(archivePathLabel("bundle.tar.gz"), "TAR.GZ");
  assert.equal(archivePathLabel("bundle.TGZ"), "TAR.GZ");
  assert.equal(archivePathLabel("document.gz"), "GZIP");
  for (const path of ["bundle.tar.bz2", "bundle.rar", "bundle"]) {
    assert.equal(isSupportedArchivePath(path), false, path);
    assert.equal(isPathSupportedForOperation(path, "extractArchive"), false, path);
  }
});
