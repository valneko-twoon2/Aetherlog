// Kinds the server does not know are shown by number only.
export const fallbackChannel = (k) => ({
  id: k,
  name: `種別 ${k.toString(16).toUpperCase().padStart(2, '0')}`,
  group: 'other',
  color: '#d8d2c0',
  layout: '{m}',
})
