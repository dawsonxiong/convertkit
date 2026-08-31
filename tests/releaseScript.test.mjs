import assert from "node:assert/strict";
import { chmodSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";

const RELEASE_SCRIPT = resolve("scripts/release-macos.sh");

function executable(path, contents) {
  writeFileSync(path, `#!/bin/sh\nset -eu\n${contents}\n`);
  chmodSync(path, 0o755);
}

function releaseFixture() {
  const root = mkdtempSync(join(tmpdir(), "convertkit-release-test-"));
  const bin = join(root, "bin");
  const temporary = join(root, "temporary");
  mkdirSync(bin);
  mkdirSync(temporary);

  executable(
    join(bin, "op"),
    String.raw`
if [ "$1" = "account" ]; then exit 0; fi
[ "$1" = "read" ] || exit 2
shift
output_file=""
reference=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --out-file) output_file=$2; shift 2 ;;
    --file-mode) shift 2 ;;
    --force|--no-newline) shift ;;
    *) reference=$1; shift ;;
  esac
done
case "$reference" in
  op://Vault/Release/signing) value='Developer ID Application: ConvertKit Test (ABCDE12345)' ;;
  op://Vault/Release/issuer) value='12345678-1234-1234-1234-123456789012' ;;
  op://Vault/Release/key-id) value='ABC123DEF4' ;;
  op://Vault/Release/private-key) value='-----BEGIN PRIVATE KEY-----' ;;
  *) exit 3 ;;
esac
if [ -n "$output_file" ]; then printf '%s\n' "$value" > "$output_file"; else printf '%s' "$value"; fi`,
  );
  executable(
    join(bin, "security"),
    `printf '%s\\n' '1) ABCDEF "Developer ID Application: ConvertKit Test (ABCDE12345)"'`,
  );
  for (const command of ["pnpm", "codesign", "xcrun", "spctl"]) {
    executable(join(bin, command), "exit 0");
  }

  const env = {
    ...process.env,
    PATH: `${bin}:${process.env.PATH ?? ""}`,
    TMPDIR: temporary,
    CONVERTKIT_SIGNING_IDENTITY_REF: "op://Vault/Release/signing",
    CONVERTKIT_APPLE_API_ISSUER_REF: "op://Vault/Release/issuer",
    CONVERTKIT_APPLE_API_KEY_ID_REF: "op://Vault/Release/key-id",
    CONVERTKIT_APPLE_API_PRIVATE_KEY_REF: "op://Vault/Release/private-key",
  };

  return { root, temporary, env };
}

test("validates release references without leaking or retaining the private key", () => {
  const fixture = releaseFixture();
  try {
    const result = spawnSync("/bin/sh", [RELEASE_SCRIPT, "--check"], {
      encoding: "utf8",
      env: fixture.env,
    });

    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /references resolve/);
    assert.deepEqual(readdirSync(fixture.temporary), []);
    assert.doesNotMatch(`${result.stdout}${result.stderr}`, /BEGIN PRIVATE KEY/);
  } finally {
    rmSync(fixture.root, { recursive: true, force: true });
  }
});

test("rejects plaintext release configuration before reading secrets", () => {
  const fixture = releaseFixture();
  try {
    const result = spawnSync("/bin/sh", [RELEASE_SCRIPT, "--check"], {
      encoding: "utf8",
      env: {
        ...fixture.env,
        CONVERTKIT_SIGNING_IDENTITY_REF: "Developer ID Application: plaintext",
      },
    });

    assert.equal(result.status, 1);
    assert.match(result.stderr, /must be an op:\/\/ secret reference/);
    assert.deepEqual(readdirSync(fixture.temporary), []);
  } finally {
    rmSync(fixture.root, { recursive: true, force: true });
  }
});
