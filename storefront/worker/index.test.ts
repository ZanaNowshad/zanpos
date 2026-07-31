// @vitest-environment node
import { describe, expect, it } from "vitest";
import worker, { signRequest, type Env } from "./index";

class MemoryR2 {
  values = new Map<string, { body: ArrayBuffer; httpMetadata?: R2HTTPMetadata; customMetadata?: Record<string, string> }>();
  async get(key: string) {
    const value = this.values.get(key);
    if (!value) return null;
    return {
      ...value,
      body: new Blob([value.body]).stream(),
      httpEtag: `"${key}"`,
      arrayBuffer: async () => value.body,
      json: async <T,>() => JSON.parse(new TextDecoder().decode(value.body)) as T,
      writeHttpMetadata: () => undefined,
    };
  }
  async put(key: string, body: ArrayBuffer | string, options?: R2PutOptions) {
    const data = typeof body === "string" ? new TextEncoder().encode(body).buffer : body;
    const httpMetadata = options?.httpMetadata instanceof Headers ? undefined : options?.httpMetadata;
    this.values.set(key, { body: data, httpMetadata, customMetadata: options?.customMetadata });
    return {} as R2Object;
  }
}

const secret = "test-secret";
const signed = async (url: string, body: string, method = "PUT") => {
  const timestamp = String(Math.floor(Date.now() / 1000));
  const signature = await signRequest(secret, timestamp, method, new URL(url).pathname, body);
  return new Request(url, {
    method,
    body,
    headers: {
      "content-type": "application/json",
      "x-zanpos-timestamp": timestamp,
      "x-zanpos-signature": signature,
      "x-idempotency-key": "release-1",
    },
  });
};

const signedImage = async (url: string, body: ArrayBuffer, contentType: string) => {
  const timestamp = String(Math.floor(Date.now() / 1000));
  const signature = await signRequest(secret, timestamp, "PUT", new URL(url).pathname, body);
  return new Request(url, {
    method: "PUT",
    body,
    headers: {
      "content-type": contentType,
      "x-zanpos-timestamp": timestamp,
      "x-zanpos-signature": signature,
      "x-idempotency-key": "image-1",
    },
  });
};

describe("catalog publishing worker", () => {
  it("rejects missing and stale HMAC authentication", async () => {
    const env = { CATALOG: new MemoryR2(), PUBLISH_SECRET: secret } as unknown as Env;
    const missing = await worker.fetch(new Request("https://shop.test/api/publish/catalog/v1", { method: "PUT" }), env);
    expect(missing.status).toBe(401);
  });

  it("publishes a validated version and commits it idempotently", async () => {
    const bucket = new MemoryR2();
    const env = { CATALOG: bucket, PUBLISH_SECRET: secret } as unknown as Env;
    const body = JSON.stringify({
      version: "v1",
      updatedAt: new Date().toISOString(),
      store: { name: { en: "Shop", ar: "متجر" }, phone: "97330000000" },
      currency: { code: "BHD", decimals: 3 },
      categories: [],
      products: [],
    });
    expect((await worker.fetch(await signed("https://shop.test/api/publish/catalog/v1", body), env)).status).toBe(201);
    const commit = await signed("https://shop.test/api/publish/releases/v1/commit", "{}", "POST");
    expect((await worker.fetch(commit, env)).status).toBe(200);
    const repeated = await signed("https://shop.test/api/publish/releases/v1/commit", "{}", "POST");
    expect(await (await worker.fetch(repeated, env)).json()).toMatchObject({ alreadyCommitted: true });
    const publicRead = await worker.fetch(new Request("https://shop.test/api/catalog"), env);
    expect(publicRead.status).toBe(200);
    expect(await publicRead.json()).toMatchObject({ version: "v1" });
  });

  it("rejects mismatched release versions before writing", async () => {
    const env = { CATALOG: new MemoryR2(), PUBLISH_SECRET: secret } as unknown as Env;
    const body = JSON.stringify({ version: "other" });
    expect((await worker.fetch(await signed("https://shop.test/api/publish/catalog/v1", body), env)).status).toBe(400);
  });

  it("rejects image payloads whose bytes do not match the declared media type", async () => {
    const env = { CATALOG: new MemoryR2(), PUBLISH_SECRET: secret } as unknown as Env;
    const bytes = new TextEncoder().encode("not really a png").buffer;
    const request = await signedImage(
      "https://shop.test/api/publish/images/fake-image",
      bytes,
      "image/png",
    );
    expect((await worker.fetch(request, env)).status).toBe(415);
  });
});
