import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  ARCHIVE_FILE_ASSOCIATIONS,
  verifyMacInfoPlist,
  verifyTauriFileAssociations,
} from "../scripts/verify-file-associations.mjs";

const tauriConfig = JSON.parse(readFileSync("src-tauri/tauri.conf.json", "utf8"));

test("declares every supported archive type as a macOS Viewer association", () => {
  assert.doesNotThrow(() => verifyTauriFileAssociations(tauriConfig));
});

test("verifies archive associations emitted into the packaged Info.plist", () => {
  const plist = {
    CFBundleDocumentTypes: ARCHIVE_FILE_ASSOCIATIONS.map((association) => ({
      CFBundleTypeExtensions: association.extensions,
      CFBundleTypeName: association.name,
      CFBundleTypeRole: "Viewer",
      LSItemContentTypes: association.contentTypes,
    })),
  };

  assert.doesNotThrow(() => verifyMacInfoPlist(plist));
  delete plist.CFBundleDocumentTypes[2].LSItemContentTypes;
  assert.throws(() => verifyMacInfoPlist(plist), /7Z Archive content types changed/);
});
