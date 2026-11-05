import { IconButton } from "@chakra-ui/react";
import { useRef } from "react";
import { LuUpload } from "react-icons/lu";

/** Picks a local parquet file and hands it to `onFile`. */
export function UploadButton({
  disabled,
  onFile,
}: {
  disabled: boolean;
  onFile: (file: File) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  return (
    <>
      <input
        ref={input}
        type="file"
        accept=".parquet,.geoparquet"
        hidden
        onChange={(e) => {
          const file = e.target.files?.[0];
          e.target.value = "";
          if (file) onFile(file);
        }}
      />
      <IconButton
        variant="outline"
        disabled={disabled}
        onClick={() => input.current?.click()}
      >
        <LuUpload aria-hidden />
      </IconButton>
    </>
  );
}
