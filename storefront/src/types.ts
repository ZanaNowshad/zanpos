export type Localized = { en: string; ar: string };

export type CatalogProduct = {
  id: string;
  name: Localized;
  description?: Localized;
  categoryId: string;
  priceMinor: number;
  available: boolean;
  quantityDecimals?: number;
  imageUrl?: string;
};

export type Catalog = {
  version: string;
  updatedAt: string;
  store: { name: Localized; phone: string; tagline?: Localized };
  currency: { code: string; decimals: number };
  categories: Array<{ id: string; name: Localized }>;
  products: CatalogProduct[];
};

export type CartLine = {
  id: string;
  name: string;
  quantity: number;
  priceMinor: number;
};

export type Locale = "en" | "ar";
