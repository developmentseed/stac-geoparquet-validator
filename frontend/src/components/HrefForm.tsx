import { Button, Input, HStack } from "@chakra-ui/react";
import { useState, type FormEvent } from "react";

/** A form for entering an href to validate. */
export function HrefForm({
  initial,
  disabled,
  onSubmit,
}: {
  initial: string;
  disabled: boolean;
  onSubmit: (href: string) => void;
}) {
  const [value, setValue] = useState(initial);
  const submit = (e: FormEvent) => {
    e.preventDefault();
    const href = value.trim();
    if (href) onSubmit(href);
  };
  return (
    <form onSubmit={submit} style={{ flex: 1 }}>
      <HStack>
        <Input
          aria-label="href"
          placeholder="https://… or s3://… stac-geoparquet file"
          value={value}
          onChange={(e) => setValue(e.target.value)}
        />
        <Button type="submit" disabled={disabled || !value.trim()}>
          Validate
        </Button>
      </HStack>
    </form>
  );
}
