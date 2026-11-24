export type ProviderRow = {
  id: string;
  label: string;
  verified: boolean;
  selected: boolean;
  endpoint: string | null;
  loopback: boolean;
  requires_key: boolean;
  endpoint_kind?: "fixed" | "choice" | "user";
  configured_endpoint?: string;
  models: string[];
  model_list_fetch: boolean;
};

export function visibleProviderRows(rows: ProviderRow[], showUntested: boolean): ProviderRow[];
export function isLoopbackEndpoint(row: Partial<ProviderRow> | null | undefined): boolean;
export function endpointClassLabel(row: Partial<ProviderRow> | null | undefined, copy: string): string;
export function shouldWarnForEndpointChange(currentEndpoint: string | undefined, nextEndpoint: string): boolean;
export function availableModels(providerId: string, known?: string[], fetched?: string[]): string[];
export function canEnableAi(input: { model: string; consent: boolean; requiresKey: boolean; keySet: boolean }): boolean;
export function providerErrorKind(error: unknown): string;
export function shouldOfferSaveAsNote(input: { answer: string; savedNoteId: string | null; noteFallback: boolean }): boolean;
