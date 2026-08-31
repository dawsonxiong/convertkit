import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { restoreWorkspaceSessions } from "../src/hooks/useWorkspaceRecovery.ts";
import { addPathsToOperation } from "../src/hooks/useAddPaths.ts";
import { parseWorkspaceDraft, WORKSPACE_DRAFT_VERSION } from "../src/lib/workspaceDraft.ts";
import { runQueue } from "../src/lib/runQueue.ts";
import { useAppStore } from "../src/store/useAppStore.ts";

function image(path) {
  return {
    path,
    name: path.split("/").pop(),
    extension: "png",
    size: 2048,
    category: "image",
    format: "png",
    width: 1200,
    height: 942,
    mimeType: "image/png",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

function requestFor(file, jobId = null) {
  return {
    operation: "convert",
    inputPath: file.path,
    jobId,
    outputFormat: "jpeg",
    outputOptions: { directory: null, suffix: "" },
  };
}

function availableCapability(inputPath) {
  return {
    inputPath,
    available: true,
    engine: "test",
    missingTools: [],
    issue: null,
    message: null,
  };
}

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

function releaseAndReset() {
  const current = useAppStore.getState();
  if (current.workspaceAdmission.token) {
    current.releaseWorkspaceClaim(current.workspaceAdmission.token);
  }
  useAppStore.getState().reset();
  if (useAppStore.getState().operation !== "convert") {
    useAppStore.getState().setOperation("convert");
  }
  useAppStore.getState().reset();
}

test("preflight atomically owns one operation and rejects a rapid second run", async () => {
  releaseAndReset();
  const source = image("/tmp/preflight-source.png");
  const racingIntake = image("/tmp/preflight-racing.png");
  useAppStore.getState().addFiles([source]);

  const capabilityGate = deferred();
  let capabilityChecks = 0;
  let executions = 0;
  const first = runQueue([source], requestFor, undefined, {
    dependencies: {
      checkCapabilities: async () => {
        capabilityChecks += 1;
        return capabilityGate.promise;
      },
      executeJob: async () => {
        executions += 1;
        return {
          output_path: "/tmp/preflight-source.jpg",
          output_paths: ["/tmp/preflight-source.jpg"],
          output_size: 1024,
          duration_ms: 12,
        };
      },
      createId: (() => {
        const ids = ["run-a", "job-a"];
        return () => ids.shift();
      })(),
    },
  });

  assert.deepEqual(useAppStore.getState().workspaceAdmission, {
    phase: "preflighting",
    token: "run-a",
    operation: "convert",
  });

  await runQueue([source], requestFor, undefined, {
    dependencies: {
      checkCapabilities: async () => {
        capabilityChecks += 1;
        return [availableCapability(source.path)];
      },
      executeJob: async () => {
        throw new Error("second run must not execute");
      },
      createId: () => "run-b",
    },
  });
  useAppStore.getState().setOperation("resize");
  useAppStore.getState().addFiles([racingIntake]);
  useAppStore.getState().setOutputFormat("webp", source.path);
  useAppStore.getState().startBatch([source.path]);
  useAppStore.getState().reset();

  assert.equal(capabilityChecks, 1);
  assert.equal(useAppStore.getState().state, "loaded");
  assert.equal(useAppStore.getState().operation, "convert");
  assert.deepEqual(
    useAppStore.getState().files.map((file) => file.path),
    [source.path],
  );
  assert.equal(useAppStore.getState().outputFormats[source.path], "jpg");

  capabilityGate.resolve([availableCapability(source.path)]);
  await first;

  assert.equal(executions, 1);
  assert.equal(useAppStore.getState().state, "done");
  assert.equal(useAppStore.getState().workspaceAdmission.phase, "idle");
  releaseAndReset();
});

test("unavailable and failed capability probes release the preflight claim", async () => {
  for (const failure of ["unavailable", "throw"]) {
    releaseAndReset();
    const source = image(`/tmp/${failure}.png`);
    useAppStore.getState().addFiles([source]);
    let executed = false;

    const originalConsoleError = console.error;
    if (failure === "throw") console.error = () => {};
    try {
      await runQueue([source], requestFor, undefined, {
        dependencies: {
          checkCapabilities: async () => {
            if (failure === "throw") throw new Error("probe failed");
            return [
              {
                ...availableCapability(source.path),
                available: false,
                issue: "missingDependency",
                message: "Missing test helper",
              },
            ];
          },
          executeJob: async () => {
            executed = true;
            throw new Error("unavailable work must not execute");
          },
          createId: () => `run-${failure}`,
        },
      });
    } finally {
      console.error = originalConsoleError;
    }

    assert.equal(executed, false);
    assert.equal(useAppStore.getState().state, "loaded");
    assert.equal(useAppStore.getState().workspaceAdmission.phase, "idle");
    assert.match(
      useAppStore.getState().rejectionMessage,
      failure === "throw" ? /verify the required local tools/i : /missing test helper/i,
    );
  }
  releaseAndReset();
});

test("a stale capability continuation cannot start work in a newer session", async () => {
  releaseAndReset();
  const source = image("/tmp/stale-preflight.png");
  useAppStore.getState().addFiles([source]);
  const capabilityGate = deferred();
  let executed = false;
  const pending = runQueue([source], requestFor, undefined, {
    dependencies: {
      checkCapabilities: async () => capabilityGate.promise,
      executeJob: async () => {
        executed = true;
        throw new Error("stale work must not execute");
      },
      createId: () => "stale-run",
    },
  });

  assert.equal(useAppStore.getState().releaseWorkspaceClaim("stale-run"), true);
  useAppStore.getState().reset();
  useAppStore.getState().setOperation("resize");
  capabilityGate.resolve([availableCapability(source.path)]);
  await pending;

  assert.equal(executed, false);
  assert.equal(useAppStore.getState().operation, "resize");
  assert.equal(useAppStore.getState().state, "empty");
  releaseAndReset();
});

test("restore owns the workspace until every saved path is revalidated", async () => {
  releaseAndReset();
  const restoredFile = image("/tmp/restored.png");
  const racingFile = image("/tmp/restore-racing.png");
  const draft = parseWorkspaceDraft(
    JSON.stringify({
      version: WORKSPACE_DRAFT_VERSION,
      activeOperation: "convert",
      sessions: { convert: { paths: [restoredFile.path] } },
    }),
  );
  assert.ok(draft);
  const collectionGate = deferred();
  const token = useAppStore.getState().beginWorkspaceRestore();
  assert.ok(token);
  const restoring = restoreWorkspaceSessions(draft, token, async () => collectionGate.promise);

  useAppStore.getState().addFiles([racingFile]);
  useAppStore.getState().setOperation("resize");
  useAppStore.getState().setOutputSuffix("convert", "-racing");
  assert.equal(useAppStore.getState().workspaceAdmission.phase, "restoring");
  assert.equal(useAppStore.getState().operation, "convert");
  assert.deepEqual(useAppStore.getState().files, []);

  collectionGate.resolve({ files: [restoredFile], skippedCount: 0, truncated: false });
  assert.equal(await restoring, true);
  assert.equal(useAppStore.getState().workspaceAdmission.phase, "idle");
  assert.deepEqual(
    useAppStore.getState().files.map((file) => file.path),
    [restoredFile.path],
  );
  assert.notEqual(useAppStore.getState().outputSuffixes.convert, "-racing");
  releaseAndReset();
});

test("intake is rejected before collection and after an intervening preflight claim", async () => {
  releaseAndReset();
  const source = image("/tmp/intake-source.png");
  const addition = image("/tmp/intake-addition.png");
  useAppStore.getState().addFiles([source]);

  const restoreToken = useAppStore.getState().beginWorkspaceRestore();
  let collectedWhileRestoring = false;
  await addPathsToOperation([addition.path], "convert", async () => {
    collectedWhileRestoring = true;
    return { files: [addition], skippedCount: 0, truncated: false };
  });
  assert.equal(collectedWhileRestoring, false);
  assert.equal(useAppStore.getState().releaseWorkspaceClaim(restoreToken), true);

  const collectionGate = deferred();
  const pendingIntake = addPathsToOperation(
    [addition.path],
    "convert",
    async () => collectionGate.promise,
  );
  assert.equal(useAppStore.getState().claimWorkspacePreflight("convert", "intake-race"), true);
  collectionGate.resolve({ files: [addition], skippedCount: 0, truncated: false });
  await pendingIntake;

  assert.deepEqual(
    useAppStore.getState().files.map((file) => file.path),
    [source.path],
  );
  assert.equal(useAppStore.getState().releaseWorkspaceClaim("intake-race"), true);
  releaseAndReset();
});

test("the app gates all intake surfaces until workspace recovery completes", async () => {
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8");
  const queue = await readFile(
    new URL("../src/components/WorkspaceQueue.tsx", import.meta.url),
    "utf8",
  );

  assert.match(app, /isInputIntakeBlocked\(state, auxiliaryViewActive, admissionPending\)/);
  assert.match(app, /workspaceAdmissionPending: admissionPending/);
  assert.match(app, /disabled=\{!workspaceReady \|\| workspaceAdmission\.phase !== "idle"\}/);
  assert.match(app, /interactionBlocked=\{admissionPending\}/);
  assert.match(queue, /<fieldset disabled=\{interactionBlocked\}/);
});
