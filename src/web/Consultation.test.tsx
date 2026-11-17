import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import demo from "./__fixtures__/hotel-demo.json";
import { PageConsultation } from "./Consultation";
import { lireCle, preparerCles } from "./crypto";

/** Le relais, servi depuis l'hôtel de démonstration envoyé par le vrai Tanana. */
async function relais() {
  const { jetonLecture } = await preparerCles(lireCle(demo.cle_secrete));
  const appels: string[] = [];
  const lecteur = (async (adresse: string, init?: RequestInit) => {
    appels.push(adresse);
    const jeton = (init?.headers as Record<string, string>).Authorization?.replace("Bearer ", "");
    if (!adresse.startsWith(`/v1/hotels/${demo.hotel_id}/`)) return new Response("{}", { status: 404 });
    if (jeton !== jetonLecture) return new Response("{}", { status: 403 });
    if (adresse.endsWith("/temoin")) return Response.json(demo.temoin);
    const apres = Number(new URL(adresse, "http://x").searchParams.get("apres"));
    return Response.json({
      lots: demo.pages.flatMap((p) => p.lots).filter((l) => l.numero > apres),
      suite: false,
      dernier_contact: new Date().toISOString(),
    });
  }) as typeof fetch;
  return { lecteur, appels };
}

async function ouvrir(cle = demo.cle_secrete, identifiant = demo.hotel_id) {
  const r = await relais();
  render(<PageConsultation lecteur={r.lecteur} />);
  await userEvent.type(screen.getByLabelText("Identifiant de l'hôtel"), identifiant);
  await userEvent.type(screen.getByLabelText("Clé secrète"), cle);
  await userEvent.click(screen.getByRole("button", { name: "Voir mon hôtel" }));
  return r;
}

describe("Page web de consultation", () => {
  beforeEach(() => {
    vi.useFakeTimers({ shouldAdvanceTime: true, now: new Date(`${demo.aujourdhui}T12:00:00`) });
  });
  afterEach(() => vi.useRealTimers());

  it("avec l'identifiant et la clé, on voit l'hôtel du jour", async () => {
    await ouvrir(demo.cle_secrete.toLowerCase(), demo.hotel_id.toLowerCase().replace("-", " "));
    expect(await screen.findByRole("heading", { name: "Hôtel du Lac" })).toBeInTheDocument();
    expect(screen.getByText(/lecture seule/)).toBeInTheDocument();
    const chambres = screen.getByRole("heading", { name: "Les chambres" }).closest("section")!;
    expect(within(chambres).getByText("Occupée")).toBeInTheDocument();
    expect(within(chambres).getByText("En maintenance")).toBeInTheDocument();
    expect(screen.getByText("Arrivées du jour").closest("section")).toHaveTextContent("Rakoto Hery");
  });

  it("une mauvaise clé donne un message clair, sans rien ouvrir", async () => {
    const r = await ouvrir("AAAA-AAAA-AAAA-AAAA-AAAA-AAAA-AA");
    expect(await screen.findByRole("alert")).toHaveTextContent("ne correspond pas à cet hôtel");
    expect(screen.queryByRole("heading", { name: "Hôtel du Lac" })).not.toBeInTheDocument();
    expect(r.appels.some((a) => a.includes("/lots"))).toBe(false);
  });

  it("une clé mal recopiée est signalée avant tout appel au serveur", async () => {
    const r = await ouvrir("ABCD-EFGH");
    expect(await screen.findByRole("alert")).toHaveTextContent("mal recopiée");
    expect(r.appels).toEqual([]);
  });

  it("un identifiant inconnu est signalé", async () => {
    await ouvrir(demo.cle_secrete, "ZZZZ-ZZZZ");
    expect(await screen.findByRole("alert")).toHaveTextContent("identifiant d'hôtel est inconnu");
  });

  it("les rubriques Réservations, Caisse et Historique", async () => {
    await ouvrir();
    await screen.findByRole("heading", { name: "Hôtel du Lac" });
    await userEvent.click(screen.getByRole("tab", { name: "Réservations" }));
    expect(screen.getByRole("table")).toHaveTextContent("Diallo Fatou");
    await userEvent.type(screen.getByLabelText("Chercher un client ou une chambre"), "rakoto");
    expect(screen.getAllByRole("row")).toHaveLength(2); // en-tête + Rakoto
    await userEvent.click(screen.getByRole("tab", { name: "Caisse" }));
    expect(screen.getByText("Reste en caisse")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("tab", { name: "Historique" }));
    await userEvent.click(screen.getByLabelText("Seulement les actions à vérifier"));
    expect(screen.getAllByText("À vérifier").length).toBeGreaterThan(0);
    expect(screen.queryByText("Client créé")).not.toBeInTheDocument();
  });

  it("Fermer efface tout et revient à la saisie", async () => {
    await ouvrir();
    await screen.findByRole("heading", { name: "Hôtel du Lac" });
    await userEvent.click(screen.getByRole("button", { name: "Fermer" }));
    expect(screen.getByRole("heading", { name: "Consulter mon hôtel" })).toBeInTheDocument();
    expect(screen.getByLabelText("Clé secrète")).toHaveValue("");
  });

  it("s'actualise seule chaque minute", async () => {
    const r = await ouvrir();
    await screen.findByRole("heading", { name: "Hôtel du Lac" });
    const avant = r.appels.length;
    await vi.advanceTimersByTimeAsync(61_000);
    expect(r.appels.length).toBeGreaterThan(avant);
  });
});
