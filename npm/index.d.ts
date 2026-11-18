export interface ValidationError {
  /** The Open XML SDK's error id, e.g. "Sch_UnexpectedElementContentExpectingComplex". */
  id: string;
  description: string;
  /** The part, e.g. "/word/document.xml" or "/xl/worksheets/sheet1.xml". */
  part: string;
  /** The element's qualified name, e.g. "w:pPr" or "x:c". */
  node: string;
  /** "Schema", "Semantic", "Package", "MarkupCompatibility" or "Xml". */
  errorType: string;
  /** The SDK's XPath to the element, e.g. "/w:document[1]/w:body[1]/w:p[3]"; absent for package and parse errors. */
  xpath?: string;
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

/** The SDK package class to open as: WordprocessingDocument, SpreadsheetDocument or PresentationDocument. */
export type Format = "word" | "excel" | "powerpoint";

/**
 * Validates a .docx, .xlsx or .pptx as `OpenXmlValidator(FileFormatVersions.Microsoft365)`
 * would, opened as `format` (default: the format its main part's content type names).
 */
export function validate(docx: Uint8Array, options?: { maxErrors?: number; format?: Format }): Promise<Report>;
