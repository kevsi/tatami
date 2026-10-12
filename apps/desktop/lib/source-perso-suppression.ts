export async function supprimerPuisRafraichirCatalogue(
  supprimer: () => Promise<unknown>,
  rafraichirCatalogue: () => Promise<unknown>,
): Promise<void> {
  await supprimer();
  await rafraichirCatalogue();
}
