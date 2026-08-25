import { productImageSrc } from "../productImage";
import { productPlaceholderSrc } from "../productPlaceholder";

interface Props {
  imagePath?: string | null;
  /** Tints the placeholder and supplies its letter. Not every caller has one. */
  categoryName?: string | null;
  productName?: string | null;
}

/**
 * A product's picture, or a stand-in that is never mistaken for one.
 *
 * Every place that showed a product used to render the same grey box glyph when
 * there was no image, so a basket of five items looked like five copies of
 * nothing and a catalogue page looked broken. Images now arrive on their own in
 * the background, which means "no image yet" is a normal, temporary state
 * rather than a rare one — worth rendering properly rather than hiding.
 *
 * The `onError` fallback matters as much as the missing case: a remote URL that
 * has since 404'd used to hide the image entirely, revealing the same shared
 * glyph. A row whose picture failed now still looks like its category.
 */
export default function ProductThumb({ imagePath, categoryName, productName }: Props) {
  const placeholder = productPlaceholderSrc(categoryName, productName);
  const src = productImageSrc(imagePath) ?? placeholder;

  return (
    <img
      src={src}
      alt=""
      loading="lazy"
      onError={event => {
        // Guarded, or a placeholder that somehow failed would re-trigger this
        // handler against itself forever.
        if (event.currentTarget.src !== placeholder) event.currentTarget.src = placeholder;
      }}
    />
  );
}
