export const COMMAND_ERROR_LOG_MARKER =
  "[discrypt:command-error] command_error_reported";

export function logSanitizedCommandError(): void {
  console.error(COMMAND_ERROR_LOG_MARKER);
}
