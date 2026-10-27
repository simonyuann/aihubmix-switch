import { useQuery } from "@tanstack/react-query";
import { dispatch, type WalletBalance } from "@/lib/api/aihubmix";
import { WalletBalanceDisplay } from "@/components/aihubmix/BalanceDisplay";

export function TrayWalletBalance({ refreshKey }: { refreshKey: number }) {
  const { data, error } = useQuery({
    queryKey: ["tray-wallet-balance", refreshKey],
    queryFn: () => dispatch<WalletBalance>("payment-balance"),
    enabled: refreshKey > 0,
    retry: false,
    gcTime: 0,
    refetchOnWindowFocus: false,
  });
  return (
    <WalletBalanceDisplay
      compact
      wallet={data}
      error={error ? String(error) : undefined}
    />
  );
}
