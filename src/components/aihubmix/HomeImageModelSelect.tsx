import { useTranslation } from "react-i18next";
import { useId } from "react";
import { Image as ImageIcon } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { dispatch } from "@/lib/api/aihubmix";
import { Button } from "@/components/ui/button";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

const gateway = "__gateway__";

function modelName(id: string, name?: string) {
  if (name && name !== id) return name;
  if (!id.startsWith("gpt-image-")) return id;
  return id
    .slice("gpt-image-".length)
    .replace(/-([a-z])/g, (_, c: string) => " " + c.toUpperCase());
}

export function HomeImageModelSelect({
  providerId,
  groupId,
  keyId,
  model,
  disabled,
  open,
  onOpenChange,
  onChange,
}: {
  providerId: string;
  groupId: number;
  keyId?: number | null;
  model: string | null;
  disabled: boolean;
  open: boolean;
  onOpenChange: (value: boolean) => void;
  onChange: (model: string | null) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  const catalog = useQuery({
    queryKey: ["tool-image-models", providerId, keyId],
    queryFn: () =>
      dispatch<{ id: string; name: string }[]>("tool-image-models", {
        app: "codex",
        groupId,
        providerId,
      }),
    enabled: open || model !== null,
    networkMode: "always",
    staleTime: 60_000,
    retry: false,
  });
  const options = catalog.data ?? [];
  const selected = options.find((option) => option.id === model);
  const catalogReady = catalog.isSuccess && !catalog.isFetching;
  const unavailable = model !== null && catalogReady && !selected;

  return (
    <div className="aihub-image-field">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        <label
          htmlFor={id}
          className="inline-flex items-center gap-1.5 text-caption font-medium text-fg-2"
        >
          <ImageIcon aria-hidden="true" size={15} />
          {t("imageModelSelect.label")}
        </label>
        <Select
          open={open}
          onOpenChange={(nextOpen) => {
            if (nextOpen) void catalog.refetch({ cancelRefetch: false });
            onOpenChange(nextOpen);
          }}
          value={model ?? gateway}
          disabled={disabled}
          onValueChange={(value) => onChange(value === gateway ? null : value)}
        >
          <SelectTrigger id={id} className="w-auto min-w-40 max-w-full gap-4">
            <SelectValue>
              {model
                ? modelName(model, selected?.name)
                : t("imageModelSelect.gatewayDefault")}
            </SelectValue>
          </SelectTrigger>
          <SelectContent>
            <SelectItem value={gateway}>
              {t("imageModelSelect.gatewayDefault")}
            </SelectItem>
            {catalogReady &&
              options.map((option) => (
                <SelectItem key={option.id} value={option.id} title={option.id}>
                  {modelName(option.id, option.name)}
                </SelectItem>
              ))}
          </SelectContent>
        </Select>
        {catalog.isFetching && (
          <span className="text-caption text-fg-3">
            {t("imageModelSelect.status.loadingModels")}
          </span>
        )}
      </div>
      {catalog.isError ? (
        <div className="mt-1 flex items-center gap-2 text-caption text-fg-3">
          <span role="alert">{t("imageModelSelect.error.loadFailed")}</span>
          <Button
            size="compact"
            variant="ghost"
            onClick={() => void catalog.refetch()}
          >
            {t("imageModelSelect.action.retry")}
          </Button>
        </div>
      ) : unavailable ? (
        <p role="alert" className="mt-1 text-caption text-destructive">
          {t("imageModelSelect.error.modelUnavailable")}
        </p>
      ) : model ? (
        <p className="mt-1 text-caption text-fg-3">
          生图和改图使用此型号。请保持 AIHub 运行。
        </p>
      ) : catalogReady && !options.length ? (
        <p className="mt-1 text-caption text-fg-3">
          当前分组仅提供网关默认选项。
        </p>
      ) : null}
    </div>
  );
}
