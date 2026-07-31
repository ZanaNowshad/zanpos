import type { StorefrontReadiness, StorefrontStatus } from "../../storefront/types";

interface Props {
  readiness: StorefrontReadiness;
  status: StorefrontStatus;
}

export default function StorefrontReadinessSummary({ readiness, status }: Props) {
  const dirtyLabel = `${status.dirty_product_count} ${
    status.dirty_product_count === 1 ? "change" : "changes"
  } waiting`;
  const liveLabel = `${status.published_product_count} ${
    status.published_product_count === 1 ? "product" : "products"
  } live`;

  return (
    <div className={`sf-readiness ${readiness.ready ? "is-ready" : "needs-work"}`}>
      <span className="sf-readiness-mark" aria-hidden="true">
        {readiness.ready ? "✓" : "!"}
      </span>
      <div>
        <strong>{readiness.ready ? "Ready to publish" : "Needs attention"}</strong>
        <span>{dirtyLabel} · {liveLabel}</span>
      </div>
      {!readiness.ready && (
        <ul>
          {readiness.issues.map(issue => <li key={issue}>{issue}</li>)}
        </ul>
      )}
    </div>
  );
}
