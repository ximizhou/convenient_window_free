// Retain the desktop redaction/validation; identify the actual host in exported reports.
import {
  buildDiagnosticReport as desktopReport,
  type DiagnosticInput,
} from '../../desktop/src/diagnostics';
import pkg from '../package.json';
export function buildDiagnosticReport(input: DiagnosticInput) {
  return { ...desktopReport(input), host: 'dsh', adapterVersion: pkg.version };
}
