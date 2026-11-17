export interface ValidationError {
  /** The Open XML SDK's error id, e.g. "Sch_UnexpectedElementContentExpectingComplex". */
  id: string;
  description: string;
  /** The part, e.g. "/word/document.xml". */
  part: string;
  /** The element's qualified name, e.g. "w:pPr". */
  node: string;
  /** "Schema", "Semantic", "Package", "MarkupCompatibility" or "Xml". */
  errorType: string;
  /** For XML that isn't well formed: where, counting from 1. */
  line?: number;
  column?: number;
}

export interface Report {
  ok: boolean;
  /** Why the package could not be read at all; errors is then empty. */
  openError: string | null;
  errors: ValidationError[];
}

/** Validates a .docx as `OpenXmlValidator(FileFormatVersions.Microsoft365)` would. */
export function validate(docx: Uint8Array, options?: { maxErrors?: number }): Promise<Report>;
