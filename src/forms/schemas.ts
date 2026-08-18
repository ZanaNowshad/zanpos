import { z } from "zod";

export const productSchema = z.object({
  name: z.string().min(1, "Name is required").max(200),
  category: z.string().min(1, "Category is required"),
  price_minor: z.number().int().positive("Price must be positive"),
  cost_minor: z.number().int().min(0).optional(),
  barcode: z.string().max(128).optional(),
  sku: z.string().max(64).optional(),
  vat_bps: z.number().int().min(0).max(10000).optional(),
  is_active: z.boolean().optional(),
});

export type ProductFormValues = z.infer<typeof productSchema>;

export const supplierSchema = z.object({
  name: z.string().min(1).max(200),
  contact_person: z.string().max(200).optional(),
  phone: z.string().max(30).optional(),
  email: z.string().email().max(254).optional().or(z.literal("")),
  address: z.string().max(500).optional(),
  notes: z.string().max(2000).optional(),
});

export type SupplierFormValues = z.infer<typeof supplierSchema>;

export const aiProviderSchema = z.object({
  provider: z.enum(["openai", "anthropic", "gemini"]),
  api_key: z.string().min(1, "API key is required"),
  model: z.string().min(1).max(100),
  base_url: z.string().url().optional().or(z.literal("")),
});

export type AiProviderFormValues = z.infer<typeof aiProviderSchema>;
