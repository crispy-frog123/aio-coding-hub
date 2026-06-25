// Usage: Minimal plugin market source parser and listing installer.

import { useState } from "react";
import { Download, RefreshCw } from "lucide-react";
import type { PluginMarketListing } from "../../services/plugins";
import { pluginParseMarketIndex } from "../../services/plugins";
import { formatUnknownError } from "../../utils/errors";
import { Button } from "../../ui/Button";

type MarketInstallInput = {
  pluginId: string;
  downloadUrl: string;
  checksum: string;
  signature?: string | null;
  publicKey?: string | null;
  source: "market";
};

export function PluginMarketPanel({
  busy,
  onInstall,
}: {
  busy: boolean;
  onInstall: (input: MarketInstallInput) => Promise<unknown>;
}) {
  const [indexUrl, setIndexUrl] = useState("");
  const [indexJson, setIndexJson] = useState("");
  const [signature, setSignature] = useState("");
  const [listings, setListings] = useState<PluginMarketListing[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  async function handleLoadMarket() {
    setLoading(true);
    setError(null);
    try {
      const next = await pluginParseMarketIndex(
        indexJson,
        indexUrl.trim() ? indexUrl : null,
        signature.trim() ? signature : null
      );
      setListings(next);
    } catch (error) {
      setError(formatUnknownError(error));
    } finally {
      setLoading(false);
    }
  }

  return (
    <section className="space-y-3 rounded-lg border border-border bg-card p-3">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 className="text-sm font-semibold text-foreground">插件市场</h2>
          <div className="text-xs text-muted-foreground">加载索引后安装或更新市场插件。</div>
        </div>
        <Button size="sm" variant="secondary" disabled={loading || busy} onClick={handleLoadMarket}>
          {loading ? <RefreshCw className="h-3.5 w-3.5 animate-spin" /> : null}
          加载市场
        </Button>
      </div>

      <div className="grid gap-2 sm:grid-cols-2">
        <label className="grid gap-1 text-xs text-muted-foreground">
          市场索引 URL
          <input
            className="rounded-md border border-border bg-background px-2 py-1.5 text-sm text-foreground"
            value={indexUrl}
            onChange={(event) => setIndexUrl(event.target.value)}
            placeholder="https://plugins.example/index.json"
          />
        </label>
        <label className="grid gap-1 text-xs text-muted-foreground">
          索引签名
          <input
            className="rounded-md border border-border bg-background px-2 py-1.5 text-sm text-foreground"
            value={signature}
            onChange={(event) => setSignature(event.target.value)}
            placeholder="可选"
          />
        </label>
      </div>

      <label className="grid gap-1 text-xs text-muted-foreground">
        市场索引 JSON
        <textarea
          className="min-h-24 rounded-md border border-border bg-background px-2 py-1.5 font-mono text-xs text-foreground"
          value={indexJson}
          onChange={(event) => setIndexJson(event.target.value)}
          placeholder='{"plugins":[]}'
        />
      </label>

      {error ? (
        <div className="rounded-md border border-destructive/30 bg-destructive/10 px-3 py-2 text-sm text-destructive">
          市场加载失败：{error}
        </div>
      ) : null}

      {listings.length === 0 ? (
        <div className="rounded-md border border-dashed border-border px-3 py-4 text-sm text-muted-foreground">
          暂无市场条目
        </div>
      ) : (
        <div className="grid gap-2">
          {listings.map((listing) => {
            const blocked =
              listing.revoked ||
              !listing.compatible ||
              !listing.downloadUrl ||
              !listing.checksum ||
              Boolean(listing.installBlockReason);
            const actionLabel = listing.updateAvailable ? "更新" : "安装";

            return (
              <article key={listing.pluginId} className="rounded-md border border-border px-3 py-2">
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div className="min-w-0">
                    <div className="truncate text-sm font-medium text-foreground">
                      {listing.name}
                    </div>
                    <div className="font-mono text-xs text-muted-foreground">
                      {listing.pluginId}
                    </div>
                  </div>
                  <Button
                    size="sm"
                    disabled={busy || blocked}
                    onClick={() =>
                      onInstall({
                        pluginId: listing.pluginId,
                        downloadUrl: listing.downloadUrl ?? "",
                        checksum: listing.checksum ?? "",
                        signature: listing.signature,
                        publicKey: null,
                        source: "market",
                      })
                    }
                  >
                    <Download className="h-3.5 w-3.5" />
                    {actionLabel}
                  </Button>
                </div>
                <div className="mt-2 flex flex-wrap gap-2 text-xs text-muted-foreground">
                  <span>版本 {listing.latestVersion ?? "-"}</span>
                  <span>{listing.compatible ? "兼容" : "不兼容"}</span>
                  <span>{listing.revoked ? "已撤销" : "未撤销"}</span>
                  <span>{listing.signature ? "已提供签名" : "未提供签名"}</span>
                </div>
                {listing.riskLabels.length > 0 ? (
                  <div className="mt-2 flex flex-wrap gap-1">
                    {listing.riskLabels.map((label) => (
                      <span
                        key={label}
                        className="rounded-md border border-border px-2 py-0.5 font-mono text-[11px] text-muted-foreground"
                      >
                        {label}
                      </span>
                    ))}
                  </div>
                ) : null}
                {listing.installBlockReason ? (
                  <div className="mt-2 text-xs text-destructive">{listing.installBlockReason}</div>
                ) : null}
              </article>
            );
          })}
        </div>
      )}
    </section>
  );
}
