import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ProductImageSearchControl } from "../components/ProductImageSearchControl";

describe("ProductImageSearchControl", () => {
  it("offers a barcode-and-name search when the product has no image", () => {
    const html = renderToStaticMarkup(
      <ProductImageSearchControl
        productName="Almarai Full Fat Milk"
        barcode="6281007023028"
        sku="MILK-1L"
        categoryName="Dairy"
        imagePath=""
        fetchLabel="Fetch image"
        changeLabel="Change image"
        searchingLabel="Finding image…"
        evidenceLabel="Search using"
        noImageLabel="No image"
        onSearch={() => {}}
      />,
    );

    expect(html).toContain("Fetch image");
    expect(html).toContain("6281007023028");
    expect(html).toContain("Almarai Full Fat Milk");
    expect(html).toContain("Search using");
  });

  it("shows the existing product image and changes the action label", () => {
    const html = renderToStaticMarkup(
      <ProductImageSearchControl
        productName="Almarai Full Fat Milk"
        barcode="6281007023028"
        sku=""
        categoryName="Dairy"
        imagePath="https://images.example.com/milk.jpg"
        fetchLabel="Fetch image"
        changeLabel="Change image"
        searchingLabel="Finding image…"
        evidenceLabel="Search using"
        noImageLabel="No image"
        onSearch={() => {}}
      />,
    );

    expect(html).toContain("Change image");
    expect(html).toContain('src="https://images.example.com/milk.jpg"');
    expect(html).not.toContain(">Fetch image<");
  });

  it("prevents an empty product search", () => {
    const html = renderToStaticMarkup(
      <ProductImageSearchControl
        productName=""
        barcode=""
        sku=""
        categoryName=""
        imagePath=""
        fetchLabel="Fetch image"
        changeLabel="Change image"
        searchingLabel="Finding image…"
        evidenceLabel="Search using"
        noImageLabel="No image"
        onSearch={() => {}}
      />,
    );

    expect(html).toContain("disabled");
  });

  it("requires both the product name and barcode", () => {
    const withoutBarcode = renderToStaticMarkup(
      <ProductImageSearchControl
        productName="Almarai Full Fat Milk"
        barcode=""
        imagePath=""
        fetchLabel="Fetch image"
        changeLabel="Change image"
        searchingLabel="Finding image…"
        evidenceLabel="Search using"
        noImageLabel="No image"
        onSearch={() => {}}
      />,
    );
    const withoutName = renderToStaticMarkup(
      <ProductImageSearchControl
        productName=""
        barcode="6281007023028"
        imagePath=""
        fetchLabel="Fetch image"
        changeLabel="Change image"
        searchingLabel="Finding image…"
        evidenceLabel="Search using"
        noImageLabel="No image"
        onSearch={() => {}}
      />,
    );

    expect(withoutBarcode).toContain("disabled");
    expect(withoutName).toContain("disabled");
  });
});
