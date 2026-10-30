import { useQueryClient } from "@tanstack/react-query";
import { useEffect } from "react";
import { ipc } from "../../lib/ipc";

/**
 * Catches up with sessions moved or renamed in Finder: at launch, and whenever the
 * window comes back to the front, which is when such changes have just happened.
 */
export function useVaultSync() {
  const queryClient = useQueryClient();
  useEffect(() => {
    const sync = async () => {
      try {
        if ((await ipc.syncVault()) > 0) {
          await Promise.all(
            [["sessions"], ["folders"], ["session"]].map((queryKey) =>
              queryClient.invalidateQueries({ queryKey }),
            ),
          );
        }
      } catch (e) {
        console.warn("vault sync failed", e);
      }
    };
    void sync();
    window.addEventListener("focus", sync);
    return () => window.removeEventListener("focus", sync);
  }, [queryClient]);
}
