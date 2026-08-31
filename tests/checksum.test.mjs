import assert from "node:assert/strict";
import test from "node:test";

import {
  checksumAlgorithmLabel,
  checksumManifestInputs,
  checksumManifestSummary,
  checksumMatches,
  parseExpectedChecksum,
} from "../src/lib/checksum.ts";
import { useInspectStore } from "../src/store/useInspectStore.ts";

const hashes = {
  md5: "900150983cd24fb0d6963f7d28e17f72",
  sha1: "a9993e364706816aba3e25717850c26c9cd0d89d",
  sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
};

test("parses raw and common checksum-manifest lines", () => {
  assert.deepEqual(parseExpectedChecksum(hashes.md5.toUpperCase()), {
    algorithm: "md5",
    digest: hashes.md5,
  });
  assert.deepEqual(parseExpectedChecksum(`SHA256: ${hashes.sha256}`), {
    algorithm: "sha256",
    digest: hashes.sha256,
  });
  assert.deepEqual(parseExpectedChecksum(`SHA-1 (archive.zip) = ${hashes.sha1}`), {
    algorithm: "sha1",
    digest: hashes.sha1,
  });
  assert.deepEqual(parseExpectedChecksum(`${hashes.sha256}  archive.zip`), {
    algorithm: "sha256",
    digest: hashes.sha256,
  });
});

test("rejects malformed, multiline, and mislabeled checksums", () => {
  assert.equal(parseExpectedChecksum("not a checksum"), null);
  assert.equal(parseExpectedChecksum(`${hashes.sha1}\n${hashes.sha256}`), null);
  assert.equal(parseExpectedChecksum(`MD5: ${hashes.sha256}`), null);
  assert.equal(parseExpectedChecksum(`${hashes.sha256.slice(0, -1)}z`), null);
});

test("compares the selected algorithm case-insensitively", () => {
  const expected = parseExpectedChecksum(hashes.sha256.toUpperCase());
  assert.ok(expected);
  assert.equal(checksumAlgorithmLabel(expected.algorithm), "SHA-256");
  assert.equal(checksumMatches(expected, hashes), true);
  assert.equal(
    checksumMatches({ ...expected, digest: `0${expected.digest.slice(1)}` }, hashes),
    false,
  );
});

test("builds typed manifest inputs without inventing relative paths", () => {
  assert.deepEqual(
    checksumManifestInputs([
      { path: "/tmp/direct.txt" },
      { path: "/tmp/folder/nested.txt", relativePath: "folder/nested.txt" },
    ]),
    [
      { inputPath: "/tmp/direct.txt", relativePath: null },
      { inputPath: "/tmp/folder/nested.txt", relativePath: "folder/nested.txt" },
    ],
  );
});

test("summarizes per-entry manifest verification outcomes", () => {
  assert.deepEqual(
    checksumManifestSummary([
      { status: "match" },
      { status: "mismatch" },
      { status: "missing" },
      { status: "match" },
    ]),
    { match: 2, mismatch: 1, missing: 1 },
  );
});

test("retains manifest work and results outside the mounted Inspect workspace", () => {
  useInspectStore.setState({ manifestTask: null, manifestView: null, manifestError: null });
  useInspectStore.getState().startManifest("verify", "manifest-job");
  useInspectStore.getState().updateManifestProgress("manifest-job", 42);
  assert.deepEqual(useInspectStore.getState().manifestTask, {
    action: "verify",
    jobId: "manifest-job",
    progress: 42,
  });

  useInspectStore.getState().completeManifest({
    kind: "verified",
    result: { manifestPath: "/tmp/checksums.sha256", algorithm: "sha256", entries: [] },
  });
  assert.equal(useInspectStore.getState().manifestTask, null);
  assert.equal(useInspectStore.getState().manifestView?.kind, "verified");

  useInspectStore.getState().startManifest("create", "cancelled-job");
  useInspectStore.getState().cancelManifest();
  assert.equal(useInspectStore.getState().manifestTask, null);
  useInspectStore.getState().dismissManifest();
});
