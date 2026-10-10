// Status audit 3 found `"Comparison"` missing from two hand-maintained
// TypeScript lists that were each supposed to mirror the real Rust
// `ReportKind` enum, undetected because nothing tied either list back to
// the actual source of truth. This test closes that gap for real: it
// reads `ReportKind`'s definition directly out of the Rust source file
// (not a second TypeScript description of what Rust "should" contain)
// and asserts `REPORT_KINDS` in `ipc.ts` — the one place this frontend
// now derives its own report-kind lists from — matches it exactly. A
// 7th variant added to the Rust enum and never mirrored here fails this
// test; a variant removed from the Rust enum and left behind here does
// too, in either direction.

import { readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

import { REPORT_KINDS } from "../utils/ipc";

const RUST_COMMON_TYPES_PATH = join(__dirname, "..", "..", "crates", "nikti-core", "src", "types", "common.rs");

function realReportKindVariants(): string[] {
  const source = readFileSync(RUST_COMMON_TYPES_PATH, "utf-8");
  const enumMatch = source.match(/pub enum ReportKind\s*\{([^}]*)\}/);
  if (!enumMatch) {
    throw new Error("could not find `pub enum ReportKind { ... }` in common.rs — has it moved or been renamed?");
  }
  return enumMatch[1]
    .split(",")
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

describe("ReportKind exhaustiveness (frontend mirrors the real Rust enum)", () => {
  it("REPORT_KINDS matches the real ReportKind enum in common.rs exactly, not a second hand-typed guess", () => {
    const real = realReportKindVariants();
    // Order-independent: what matters is set equality, not declaration
    // order matching between the two languages.
    expect([...REPORT_KINDS].sort()).toEqual([...real].sort());
  });

  it("would have caught the original bug: REPORT_KINDS without Comparison mismatches the real enum", () => {
    const real = realReportKindVariants();
    const preFixReportKinds = REPORT_KINDS.filter((k) => k !== "Comparison");
    expect([...preFixReportKinds].sort()).not.toEqual([...real].sort());
  });
});
