import { zodResolver } from "@hookform/resolvers/zod";
import { useForm, type UseFormProps } from "react-hook-form";
import type { ZodSchema, z } from "zod";

export function useZodForm<TSchema extends ZodSchema>(
  schema: TSchema,
  options?: Omit<UseFormProps<z.infer<TSchema>>, "resolver">
) {
  return useForm<z.infer<TSchema>>({
    resolver: zodResolver(schema),
    mode: "onBlur",
    ...options,
  });
}
