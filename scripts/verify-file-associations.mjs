#!/usr/bin/env node

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import { pathToFileURL } from "node:url";

export const ARCHIVE_FILE_ASSOCIATIONS = Object.freeze([
  {
    name: "ZIP Archive",
    extensions: ["zip"],
    contentTypes: ["public.zip-archive"],
  },
  {
    name: "TAR Archive",
    extensions: ["tar"],
    contentTypes: ["public.tar-archive"],
  },
  {
    name: "7Z Archive",
    extensions: ["7z"],
    contentTypes: ["org.7-zip.7-zip-archive"],
  },
  {
    name: "GZIP Archive",
    extensions: ["gz"],
    contentTypes: ["org.gnu.gnu-zip-archive"],
  },
  {
    name: "TGZ Archive",
    extensions: ["tgz"],
    contentTypes: ["org.gnu.gnu-zip-tar-archive"],
  },
]);

function associationNamed(associations, name) {
  const association = associations.find((candidate) => candidate.name === name);
  assert.ok(association, `missing ${name} file association`);
  return association;
}

export function verifyTauriFileAssociations(config) {
  const associations = config?.bundle?.fileAssociations;
  assert.ok(Array.isArray(associations), "bundle.fileAssociations must be an array");

  for (const expected of ARCHIVE_FILE_ASSOCIATIONS) {
    const actual = associationNamed(associations, expected.name);
    assert.deepEqual(actual.ext, expected.extensions, `${expected.name} extensions changed`);
    assert.deepEqual(
      actual.contentTypes,
      expected.contentTypes,
      `${expected.name} macOS content types changed`,
    );
    assert.equal(actual.role, "Viewer", `${expected.name} must remain a Viewer association`);
  }
}

export function verifyMacInfoPlist(plist) {
  const documentTypes = plist?.CFBundleDocumentTypes;
  assert.ok(Array.isArray(documentTypes), "CFBundleDocumentTypes must be an array");

  for (const expected of ARCHIVE_FILE_ASSOCIATIONS) {
    const actual = documentTypes.find((candidate) => candidate.CFBundleTypeName === expected.name);
    assert.ok(actual, `built app is missing ${expected.name}`);
    assert.deepEqual(
      actual.CFBundleTypeExtensions,
      expected.extensions,
      `built ${expected.name} extensions changed`,
    );
    assert.deepEqual(
      actual.LSItemContentTypes,
      expected.contentTypes,
      `built ${expected.name} content types changed`,
    );
    assert.equal(
      actual.CFBundleTypeRole,
      "Viewer",
      `built ${expected.name} must remain a Viewer association`,
    );
  }
}

function parseArguments(arguments_) {
  let configPath = "src-tauri/tauri.conf.json";
  let plistPath = null;

  for (let index = 0; index < arguments_.length; index += 1) {
    const argument = arguments_[index];
    if (argument === "--config") {
      configPath = arguments_[index + 1];
      index += 1;
    } else if (argument === "--plist") {
      plistPath = arguments_[index + 1];
      index += 1;
    } else {
      throw new Error(`unknown argument: ${argument}`);
    }
  }

  return { configPath, plistPath };
}

function run() {
  const { configPath, plistPath } = parseArguments(process.argv.slice(2));
  const config = JSON.parse(readFileSync(configPath, "utf8"));
  verifyTauriFileAssociations(config);

  if (plistPath) {
    const plistJson = execFileSync("/usr/bin/plutil", ["-convert", "json", "-o", "-", plistPath], {
      encoding: "utf8",
    });
    verifyMacInfoPlist(JSON.parse(plistJson));
  }

  process.stdout.write(
    plistPath
      ? "Source and packaged archive file associations are valid.\n"
      : "Source archive file associations are valid.\n",
  );
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  run();
}
