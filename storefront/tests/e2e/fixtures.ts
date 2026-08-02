import type { Catalog } from "../../src/types";

/**
 * Deterministic catalog fixture for Playwright E2E tests.
 * All monetary values are integer minor units (1 BHD = 1000 fils).
 * No real customer data, credentials, or production values.
 */
export function englishCatalog(): Catalog {
  return {
    version: "2026-08-02-test",
    updatedAt: new Date("2026-08-02T12:00:00Z").toISOString(),
    store: {
      name: { en: "Test Market", ar: "السوق التجريبي" },
      phone: "+97317000000",
      tagline: { en: "Fresh every day", ar: "طازج كل يوم" },
    },
    currency: { code: "BHD", decimals: 3 },
    categories: [
      { id: "cat-1", name: { en: "Dairy", ar: "ألبان" } },
      { id: "cat-2", name: { en: "Bakery", ar: "مخبوزات" } },
      { id: "cat-3", name: { en: "Beverages", ar: "مشروبات" } },
    ],
    products: [
      {
        id: "prod-1",
        name: { en: "Fresh Milk 1L", ar: "حليب طازج ١ لتر" },
        description: { en: "Full cream fresh milk", ar: "حليب كامل الدسم طازج" },
        categoryId: "cat-1",
        priceMinor: 600, // 0.600 BHD
        available: true,
        quantityDecimals: 0,
        imageUrl: undefined,
      },
      {
        id: "prod-2",
        name: { en: "Arabic Bread", ar: "خبز عربي" },
        description: { en: "Freshly baked pita", ar: "خبز بيتا طازج" },
        categoryId: "cat-2",
        priceMinor: 200, // 0.200 BHD
        available: true,
        quantityDecimals: 0,
        imageUrl: undefined,
      },
      {
        id: "prod-3",
        name: { en: "Orange Juice 500ml", ar: "عصير برتقال ٥٠٠ مل" },
        categoryId: "cat-3",
        priceMinor: 450, // 0.450 BHD
        available: true,
        imageUrl: undefined,
      },
      {
        id: "prod-4",
        name: { en: "Labneh 500g", ar: "لبنة ٥٠٠ غرام" },
        categoryId: "cat-1",
        priceMinor: 1200, // 1.200 BHD
        available: false,
        imageUrl: undefined,
      },
      {
        id: "prod-5",
        name: { en: "Saffron Cake", ar: "كعكة زعفران" },
        categoryId: "cat-2",
        priceMinor: 2500, // 2.500 BHD
        available: true,
        quantityDecimals: 0,
        imageUrl: undefined,
      },
      {
        id: "prod-6",
        name: { en: "Mineral Water 1.5L", ar: "مياه معدنية ١.٥ لتر" },
        categoryId: "cat-3",
        priceMinor: 150, // 0.150 BHD
        available: true,
        imageUrl: undefined,
      },
    ],
  };
}

export function arabicCatalog(): Catalog {
  return englishCatalog();
}

export function emptyCatalog(): Catalog {
  return {
    ...englishCatalog(),
    products: [],
  };
}

export function unavailableCatalog(): Catalog {
  return {
    ...englishCatalog(),
    products: englishCatalog().products.map((p) => ({ ...p, available: false })),
  };
}
