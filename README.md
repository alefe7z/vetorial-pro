# Vetorial

Editor de ilustração vetorial gratuito e de código aberto para Windows, em português.

O Vetorial é baseado no **[VectorCraft](https://github.com/storytold/vectorcraft)**, um editor
vetorial open source escrito em Rust, e traz todas as suas funções: caneta Bézier e seleção direta,
Pathfinder e Shape Builder, gradientes e malha de gradiente, mesclagens, distorção por envelope,
grade de perspectiva, vetorização de imagens, efeitos, camadas e pranchetas, texto com estilos,
e importação/exportação de SVG, PDF, `.ai`, EPS, DXF, PNG, JPEG e outros.

## Baixar o programa

1. Abra a aba **Actions** deste repositório.
2. Clique na execução mais recente com o ✓ verde de **Gerar EXE**.
3. Em **Artifacts**, baixe **Vetorial-windows** e extraia o zip. O programa é o `Vetorial.exe`.

Se o Windows mostrar "O Windows protegeu o computador", clique em **Mais informações** →
**Executar assim mesmo** (o programa não tem assinatura digital paga).

## Gerar uma versão nova

Toda alteração enviada para a branch `main` gera um `.exe` novo automaticamente
(`.github/workflows/gerar-exe.yml`). Para gerar sem alterar nada: **Actions** → **Gerar EXE** →
**Run workflow**. A primeira compilação leva bem mais tempo; as seguintes reaproveitam o que já foi
compilado.

## Licença e créditos

MIT OR Apache-2.0, como o projeto original (`LICENSE-MIT`, `LICENSE-APACHE`). O `NOTICE` lista as
alterações em relação ao VectorCraft e os créditos de fontes, ícones e bibliotecas. O Vetorial não é
afiliado à equipe ArtCraft; o nome e os logos ArtCraft são marcas deles e não são usados no programa.
O README original do VectorCraft está em [`README-original.md`](README-original.md).
