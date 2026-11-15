export const HUD_MOCK_INTERVAL_MS = 1_500;

export const mockHudStatePayload = {
  schema: 1,
  visible: true,
  fields: {
    kills: { value: 5, availability: "fresh" },
    deaths: { value: 2, availability: "fresh" },
    assists: { value: 8, availability: "fresh" },
    gold: { value: 1_234, availability: "fresh" },
    gpm: { value: 456, availability: "fresh" },
    xpm: { value: 789, availability: "fresh" },
  },
  net_worth: { value: null, availability: "unsupported" },
} as const;
