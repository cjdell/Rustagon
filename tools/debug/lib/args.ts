// Shared CLI arg parsing for the debug tools.
//
// Tools take a leading positional (host / file-prefix) plus optional flags
// (e.g. --timeout 5, --dir /tmp/x). A naive "first non-flag arg" picker would
// misread a flag's *value* as the host, so flags that consume a value are
// skipped together with their value.

export function parsePositional(args: string[], flagsWithValue: string[]): string[] {
  const positional: string[] = [];
  for (let i = 0; i < args.length; i++) {
    const a = args[i];
    if (a === "--") continue;
    if (flagsWithValue.includes(a)) {
      i++; // skip the flag's value
      continue;
    }
    if (a.startsWith("-")) continue; // bare flag (e.g. --raw, --changed, --ascii)
    positional.push(a);
  }
  return positional;
}

/** Value of a `--name value` flag, or undefined. */
export function getFlag(args: string[], name: string): string | undefined {
  const i = args.indexOf(name);
  return i >= 0 ? args[i + 1] : undefined;
}
