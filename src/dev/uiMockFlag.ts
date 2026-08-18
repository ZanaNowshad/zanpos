/**
 * Activation guard for the dev-only visual-QA mock.
 *
 * Deliberately contains NO mock data and NO imports of the mock module. The
 * mock itself is loaded with a dynamic `import()` behind this guard so that
 * Rollup can split it out and no fake business data reaches a production
 * bundle. Keeping the guard in its own module is what makes that possible:
 * if the data module were imported statically anywhere, its payload would
 * ship even though the behaviour is unreachable.
 */
export function uiMockEnabled(): boolean {
  return import.meta.env.DEV && new URLSearchParams(location.search).has("uimock");
}

/** Set by installUiMock() once the mock module has loaded. */
export function mockSession(): unknown {
  return (window as unknown as { __zpMockSession?: unknown }).__zpMockSession ?? null;
}
