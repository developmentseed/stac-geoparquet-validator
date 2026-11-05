import { ChakraProvider, defaultSystem } from "@chakra-ui/react";
import { cleanup, render } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { UploadButton } from "./UploadButton";

afterEach(cleanup);

it("accepts .parquet and .geoparquet files", () => {
  const { container } = render(
    <ChakraProvider value={defaultSystem}>
      <UploadButton disabled={false} onFile={() => {}} />
    </ChakraProvider>,
  );
  const accept = container
    .querySelector('input[type="file"]')
    ?.getAttribute("accept")
    ?.split(",");
  expect(accept).toContain(".parquet");
  expect(accept).toContain(".geoparquet");
});
