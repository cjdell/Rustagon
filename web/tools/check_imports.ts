import * as ts from "typescript";
import { dirname, join, normalize, relative, resolve } from "node:path";

const WEB_ROOT = resolve(Deno.cwd());
const LIB_ROOT = resolve(WEB_ROOT, "src/lib");
const CORE_ROOT = resolve(LIB_ROOT, "core");

const denoJson = JSON.parse(await Deno.readTextFile(resolve(WEB_ROOT, "deno.json")));
const imports = (denoJson.imports ?? {}) as Record<string, string>;

type Rule = "no-lib-self-import" | "core-parent-import";

interface Violation {
  file: string;
  line: number;
  specifier: string;
  rule: Rule;
  message: string;
}

const violations: Violation[] = [];

// A specifier is a local alias if deno.json maps it to a relative path (not npm:/node:).
const isLocalAlias = (spec: string): boolean => {
  const target = imports[spec];
  return target !== undefined && (target.startsWith("./") || target.startsWith("../") || target.startsWith("/"));
};

const isRelative = (spec: string): boolean => spec.startsWith("./") || spec.startsWith("../");

const isInside = (root: string, p: string): boolean => {
  const r = resolve(p);
  return r === root || r.startsWith(root + "/");
};

const relativeToWeb = (p: string): string => relative(WEB_ROOT, resolve(p));

function collectSpecifiers(sf: ts.SourceFile): Array<{ spec: string; pos: number }> {
  const out: Array<{ spec: string; pos: number }> = [];

  const visit = (node: ts.Node): void => {
    if (
      (ts.isImportDeclaration(node) || ts.isExportDeclaration(node)) &&
      node.moduleSpecifier &&
      ts.isStringLiteral(node.moduleSpecifier)
    ) {
      out.push({ spec: node.moduleSpecifier.text, pos: node.moduleSpecifier.pos });
    } else if (ts.isImportEqualsDeclaration(node)) {
      const ref = node.moduleReference;
      if (ts.isExternalModuleReference(ref) && ref.expression && ts.isStringLiteral(ref.expression)) {
        out.push({ spec: ref.expression.text, pos: ref.expression.pos });
      }
    } else if (ts.isCallExpression(node) && ts.isImportKeyword(node.expression) && node.arguments.length === 1) {
      const arg = node.arguments[0];
      if (ts.isStringLiteral(arg)) {
        out.push({ spec: arg.text, pos: arg.pos });
      }
    }

    ts.forEachChild(node, visit);
  };

  visit(sf);
  return out;
}

function check(fileAbs: string, line: number, spec: string): void {
  const file = relativeToWeb(fileAbs);

  // Rule 1: src/lib must not depend on the @lib barrel it lives inside.
  if (spec === "@lib" || spec.startsWith("@lib/")) {
    violations.push({
      file,
      line,
      specifier: spec,
      rule: "no-lib-self-import",
      message: `src/lib must not import "${spec}" — it would depend on the @lib barrel it lives inside.`,
    });
  }

  // Rule 2: src/lib/core may only import within itself, plus published npm modules.
  if (isInside(CORE_ROOT, fileAbs)) {
    if (isRelative(spec)) {
      const resolved = normalize(join(dirname(fileAbs), spec));
      if (!isInside(CORE_ROOT, resolved)) {
        violations.push({
          file,
          line,
          specifier: spec,
          rule: "core-parent-import",
          message: `src/lib/core may only import within itself; "${spec}" escapes to ${relativeToWeb(resolved)}. Only published npm modules are allowed across the boundary.`,
        });
      }
    } else if (spec !== "@lib" && isLocalAlias(spec)) {
      violations.push({
        file,
        line,
        specifier: spec,
        rule: "core-parent-import",
        message: `src/lib/core may not import the local alias "${spec}" (maps to ${imports[spec]}); only published npm modules or relative imports within core.`,
      });
    }
  }
}

async function* walkTs(dir: string): AsyncGenerator<string> {
  for await (const entry of Deno.readDir(dir)) {
    const p = join(dir, entry.name);
    if (entry.isDirectory) {
      yield* walkTs(p);
    } else if (/\.(ts|tsx)$/.test(entry.name)) {
      yield p;
    }
  }
}

if (!(await Deno.stat(LIB_ROOT).catch(() => null))) {
  console.error(`Import checker: ${relativeToWeb(LIB_ROOT)} not found — run from the web/ directory.`);
  Deno.exit(2);
}

let libFiles = 0;
let coreFiles = 0;

for await (const fileAbs of walkTs(LIB_ROOT)) {
  const text = await Deno.readTextFile(fileAbs);
  const sf = ts.createSourceFile(
    fileAbs,
    text,
    ts.ScriptTarget.Latest,
    true,
    fileAbs.endsWith(".tsx") ? ts.ScriptKind.TSX : ts.ScriptKind.TS,
  );

  libFiles++;
  if (isInside(CORE_ROOT, fileAbs)) coreFiles++;

  for (const { spec, pos } of collectSpecifiers(sf)) {
    const line = sf.getLineAndCharacterOfPosition(pos).line + 1;
    check(fileAbs, line, spec);
  }
}

violations.sort((a, b) => a.file.localeCompare(b.file) || a.line - b.line);

if (violations.length > 0) {
  console.error(`Import convention violations (${violations.length}):\n`);
  for (const v of violations) {
    console.error(`${v.file}:${v.line}  [${v.rule}]`);
    console.error(`    import "${v.specifier}" — ${v.message}\n`);
  }
  Deno.exit(1);
}

console.log(`Import conventions OK — ${libFiles} files checked in src/lib (${coreFiles} in core).`);
