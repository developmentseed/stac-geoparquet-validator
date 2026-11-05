import { HStack, Link } from "@chakra-ui/react";
import { CollecticonBrandDevelopmentSeed2 } from "@devseed-ui/collecticons-chakra";
import { LuHeart } from "react-icons/lu";

/** The page footer crediting Development Seed. */
export function Footer() {
  return (
    <HStack as="footer" fontWeight="lighter" fontSize="small" gap="1">
      Crafted with <LuHeart aria-label="love" /> by{" "}
      <Link href="https://developmentseed.org/">
        Development Seed <CollecticonBrandDevelopmentSeed2 />
      </Link>
    </HStack>
  );
}
