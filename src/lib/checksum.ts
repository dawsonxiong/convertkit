import type {
  ChecksumManifestInput,
  ChecksumManifestVerificationEntry,
  FileHashes,
  FileInfo,
} from "../types";

type ChecksumAlgorithm = keyof FileHashes;

export interface ExpectedChecksum {
  algorithm: ChecksumAlgorithm;
  digest: string;
}

const ALGORITHM_BY_LENGTH: Record<number, ChecksumAlgorithm> = {
  32: "md5",
  40: "sha1",
  64: "sha256",
};

function algorithmFromLabel(value: string): ChecksumAlgorithm | null {
  switch (value.toLowerCase().replaceAll("-", "")) {
    case "md5":
      return "md5";
    case "sha1":
      return "sha1";
    case "sha256":
      return "sha256";
    default:
      return null;
  }
}

function expectedChecksum(digest: string, labeledAlgorithm?: string): ExpectedChecksum | null {
  const normalized = digest.toLowerCase();
  const algorithm = ALGORITHM_BY_LENGTH[normalized.length] ?? null;
  if (!algorithm || !/^[a-f0-9]+$/.test(normalized)) return null;
  if (labeledAlgorithm && algorithmFromLabel(labeledAlgorithm) !== algorithm) return null;
  return { algorithm, digest: normalized };
}

export function parseExpectedChecksum(value: string): ExpectedChecksum | null {
  const input = value.trim();
  if (!input || input.includes("\n") || input.includes("\r")) return null;

  const raw = input.match(/^([a-f0-9]{32}|[a-f0-9]{40}|[a-f0-9]{64})$/i);
  if (raw) return expectedChecksum(raw[1]);

  const labeled = input.match(/^(MD5|SHA-?1|SHA-?256)\s*[:=]\s*([a-f0-9]+)$/i);
  if (labeled) return expectedChecksum(labeled[2], labeled[1]);

  const bsd = input.match(/^(MD5|SHA-?1|SHA-?256)\s*\(.+\)\s*=\s*([a-f0-9]+)$/i);
  if (bsd) return expectedChecksum(bsd[2], bsd[1]);

  const manifest = input.match(/^([a-f0-9]+)[\t ]+\*?.+$/i);
  return manifest ? expectedChecksum(manifest[1]) : null;
}

export function checksumAlgorithmLabel(algorithm: ChecksumAlgorithm): string {
  switch (algorithm) {
    case "md5":
      return "MD5";
    case "sha1":
      return "SHA-1";
    case "sha256":
      return "SHA-256";
  }
}

export function checksumMatches(expected: ExpectedChecksum, hashes: FileHashes): boolean {
  return hashes[expected.algorithm].toLowerCase() === expected.digest;
}

export function checksumManifestInputs(
  files: readonly Pick<FileInfo, "path" | "relativePath">[],
): ChecksumManifestInput[] {
  return files.map((file) => ({
    inputPath: file.path,
    relativePath: file.relativePath ?? null,
  }));
}

export function checksumManifestSummary(entries: readonly ChecksumManifestVerificationEntry[]) {
  return entries.reduce(
    (summary, entry) => ({ ...summary, [entry.status]: summary[entry.status] + 1 }),
    { match: 0, mismatch: 0, missing: 0 },
  );
}
