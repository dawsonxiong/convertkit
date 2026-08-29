import { useMemo } from "react";
import type { Operation, OutputOptions } from "../types";
import { useAppStore } from "../store/useAppStore";

export function useOutputOptions(operation: Operation): OutputOptions {
  const directory = useAppStore((state) => state.outputDirectory);
  const suffix = useAppStore((state) => state.outputSuffixes[operation]);
  const collisionPolicy = useAppStore((state) => state.collisionPolicy);

  return useMemo(
    () => ({ directory, suffix, collisionPolicy }),
    [collisionPolicy, directory, suffix],
  );
}
