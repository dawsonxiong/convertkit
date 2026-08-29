import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "../store/useAppStore";

function folderName(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function OutputSettings() {
  const operation = useAppStore((state) => state.operation);
  const directory = useAppStore((state) => state.outputDirectory);
  const suffix = useAppStore((state) => state.outputSuffixes[operation]);
  const collisionPolicy = useAppStore((state) => state.collisionPolicy);
  const setOutputDirectory = useAppStore((state) => state.setOutputDirectory);
  const setOutputSuffix = useAppStore((state) => state.setOutputSuffix);
  const setCollisionPolicy = useAppStore((state) => state.setCollisionPolicy);

  const chooseDirectory = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Choose output folder",
    });
    if (typeof selected === "string") setOutputDirectory(selected);
  };

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Output</h3>

      <div className="grid grid-cols-[78px_minmax(0,1fr)] items-center gap-x-3 gap-y-2.5">
        <label className="text-[11px] font-medium text-white/45">Folder</label>
        <div className="flex min-w-0 gap-1.5">
          <button
            type="button"
            onClick={chooseDirectory}
            title={directory ?? "Save beside each source file"}
            className="h-8 min-w-0 flex-1 truncate border border-[#44464f] bg-[#101012] px-2.5 text-left text-[11px] text-white/75 hover:border-white/25 hover:bg-[#18181b]"
          >
            {directory ? folderName(directory) : "Source folder"}
          </button>
          {directory && (
            <button
              type="button"
              onClick={() => setOutputDirectory(null)}
              className="h-8 border border-[#44464f] bg-[#201f22] px-2 text-[10px] text-white/55 hover:bg-[#2a2a2c] hover:text-white"
            >
              Reset
            </button>
          )}
        </div>

        <label htmlFor="output-suffix" className="text-[11px] font-medium text-white/45">
          Suffix
        </label>
        <input
          id="output-suffix"
          type="text"
          value={suffix}
          maxLength={80}
          spellCheck={false}
          placeholder="None"
          onChange={(event) => setOutputSuffix(operation, event.target.value)}
          className="h-8 min-w-0 border border-[#44464f] bg-[#101012] px-2.5 text-[11px] text-white/80 outline-none placeholder:text-white/25 hover:border-white/25 focus:border-[#b0c6ff]"
        />

        <label htmlFor="collision-policy" className="text-[11px] font-medium text-white/45">
          If existing
        </label>
        <div className="relative min-w-0">
          <select
            id="collision-policy"
            value={collisionPolicy}
            onChange={(event) =>
              setCollisionPolicy(event.target.value === "replace" ? "replace" : "rename")
            }
            className="h-8 w-full appearance-none border border-[#44464f] bg-[#101012] pl-2.5 pr-8 text-[11px] text-white/80 outline-none hover:border-white/25 focus:border-[#b0c6ff]"
          >
            <option value="rename">Keep both</option>
            <option value="replace">Replace existing</option>
          </select>
          <svg
            className="pointer-events-none absolute right-2.5 top-1/2 size-3 -translate-y-1/2 text-white/35"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            aria-hidden="true"
          >
            <path d="m7 9 5 5 5-5" />
          </svg>
        </div>
      </div>
    </section>
  );
}
