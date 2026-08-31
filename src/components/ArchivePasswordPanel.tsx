import { useAppStore } from "../store/useAppStore";

export function ArchivePasswordPanel() {
  const files = useAppStore((state) => state.files);
  const passwords = useAppStore((state) => state.extractArchivePasswords);
  const setPassword = useAppStore((state) => state.setExtractArchivePassword);
  const sevenZFiles = files.filter((file) => file.name.toLowerCase().endsWith(".7z"));

  if (sevenZFiles.length === 0) return null;

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">
        {sevenZFiles.length === 1 ? "7Z password" : "7Z passwords"}
      </h3>
      <div className="space-y-3">
        {sevenZFiles.map((file) => (
          <label key={file.path} className="block min-w-0">
            {sevenZFiles.length > 1 && (
              <span className="mb-1.5 block truncate text-[11px] font-medium text-white/55">
                {file.name}
              </span>
            )}
            <input
              type="password"
              value={passwords[file.path] ?? ""}
              maxLength={256}
              autoComplete="current-password"
              spellCheck={false}
              onChange={(event) => setPassword(file.path, event.target.value)}
              placeholder="Enter if required"
              aria-label={`Password for ${file.name}`}
              className="archive-password-input"
            />
          </label>
        ))}
      </div>
    </section>
  );
}
