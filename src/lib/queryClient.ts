import { QueryClient } from "@tanstack/react-query";

/** Shared so background work (like notes written after a recording) can refresh views. */
export const queryClient = new QueryClient();
