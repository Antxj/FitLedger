# FitLedger — Plano de UX e gamificação

Objetivo: deixar o app mais motivador e mais fácil de usar no dia a dia, sem atrapalhar o
treinador.

## 1. Princípios (o que a gamificação NÃO pode fazer)

- **Premiar a constância e o "treino certo", não o volume nem a intensidade.**
  - Quem corre a corrida leve dentro da FC ganha mais do que quem corre mais rápido.
  - Km extras fora do plano não dão pontos.
  - Assim a gamificação reforça o plano em vez de competir com ele.
- **Descanso também conta.**
  - A sequência é por **semana**, não por dia: dia de descanso nunca quebra nada.
  - A semana leve planejada vale como semana cumprida.
- **Sem punição.** Treino perdido não tira pontos. No máximo, a sequência recomeça.
  - Existe um "coringa" (uma semana a cada 8 pode falhar sem zerar).
  - Semana marcada como doente ou viagem não conta.
- **Competição só consigo mesmo.** O app é local, sem ranking nem rede social.
- **Toda recompensa explica o porquê**, como os ajustes do treinador já fazem.
- **Tom discreto e adulto.** Comemorar sim, infantilizar não. A animação é curta e dá para
  desligar.

Há estudos de que elementos de jogo aumentam a atividade física, com efeito pequeno a moderado
(ex.: Mazeas et al., *J Med Internet Res* 2022, revisão sistemática e meta-análise). O efeito
depende de premiar comportamentos saudáveis e realistas.

## 2. Elementos de jogo

### 2.1 Boletim pós-corrida (o ponto central)
Ao importar uma corrida, abre um cartão de resultado:
- **Até 3 estrelas:** ⭐ fez o treino (tempo ou distância) · ⭐ ficou na zona certa (ex.: menos de
  10% do tempo acima do limite numa corrida leve) · ⭐ objetivo da sessão cumprido (pace do bloco
  forte, distância do teste etc.).
- **XP ganho**, com a conta visível.
- **Comparação motivadora com evidência**, ex.: "FC 3 bpm menor que no último teste de
  evolução", "seu maior longão até agora".
- **Conquistas desbloqueadas** e o próximo treino.

### 2.2 XP e níveis
- **XP por treino do plano:** feito = 10, parcial = 5, +5 por estrela de zona e +5 por estrela de
  objetivo; teste de evolução = 20; tentativa da meta = 50.
- **Corridas fora do plano:** 5 XP fixos, para não incentivar treino extra.
- **Níveis com nomes de corrida:** Primeiros Passos → Trotador → Corredor 3K → Corredor 5K →
  Corredor 8K → Corredor 10K → Coração de Fundista…
  - A curva é suave: um iniciante que cumpre o plano sobe a cada 2 ou 3 semanas no começo.
- **XP derivado dos dados:** é recalculado a partir das corridas e do plano, igual às métricas,
  então nunca fica inconsistente.

### 2.3 Sequência semanal
- **"Semanas consistentes":** semanas com pelo menos 2/3 dos treinos do plano feitos.
- **Coringa:** 1 a cada 8 semanas. A semana leve planejada sempre conta.
- **Visual:** uma fileira de "medalhas de semana" na tela Hoje.

### 2.4 Conquistas (sala de troféus)
Uma tela nova, **Conquistas**, com troféus desbloqueados (com data e a corrida que gerou) e os
bloqueados com a dica de como ganhar. Exemplos:

| Categoria | Conquistas |
|---|---|
| Primeiros passos | Primeira corrida · Primeiro plano · Primeira semana completa |
| Distância | Primeiros 3 / 5 / 8 / 10 km contínuos |
| Coração | **Zona 2 de verdade** (5 corridas leves dentro do limite) · **Coração eficiente** (−3, −5, −10 bpm no pace da meta desde o início) · **Deriva sob controle** (longão com deriva abaixo de 5%) |
| Constância | 4 / 8 / 12 / 26 semanas consistentes · Plano de um mês inteiro cumprido |
| Records | Novo record de 1 km / 5 km / 10 km (rua ou esteira) |
| Meta | Meta atingida · Meta atingida antes da data estimada |
| Hábito | Madrugador (5 corridas antes das 7h) · Rua e esteira (as duas na mesma semana) · Respeitou a semana leve |

### 2.5 Mapa da jornada até a meta
- **O trajeto:** a meta vira um caminho visual (estilo trilha de jogo) do dia em que ela foi
  criada até a meta.
- **Marcos no caminho:** as distâncias do longão (3, 4, 5 km…), os testes de evolução com a FC de
  cada um e a tentativa final.
- **Posição:** um marcador "você está aqui" e a estimativa de chegada.
- Substitui as barras de progresso do cartão "Meta" na tela Hoje, com mais destaque.

### 2.6 Missões da semana
- **Origem:** 2 ou 3 missões geradas a partir do plano da semana, ex.: "Faça os 3 treinos",
  "Mantenha 2 corridas leves abaixo de 149 bpm", "Complete o teste de evolução".
- **Recompensa:** um bônus de XP e uma medalha de semana dourada.
- **Nunca** missões de volume extra ("corra 5 km a mais").

### 2.7 Comemorações
- **Animação curta** (partículas/confete, ~1,5 s) ao subir de nível, desbloquear conquista
  importante ou atingir a meta. Pode ser desligada nas Configurações.
- **Retrospectivas:** do mês e do ano ("seu ano na corrida"), com totais, records, a evolução da FC
  e as conquistas, exportáveis como imagem ou HTML.

## 3. Melhorias de UX (sem gamificação)

1. **Arrastar e soltar** arquivos `.fit` na janela para importar.
2. **Pasta vigiada com o app aberto:** importa assim que um `.fit` novo aparece. Hoje só importa ao
   abrir.
3. **Lembrete do treino do dia** como notificação do Windows (opcional, no horário escolhido).
4. **Explicações no próprio lugar:** ícone "?" em termos como zona 2, deriva, FC de reserva e
   teste de evolução, com uma frase simples.
5. **Tela Hoje mais direta:** um único botão principal com o próximo passo ("Importar a corrida de
   hoje", "Ver o treino", "Definir meta").
6. **Calendário mensal do plano:** a visão por semana continua, e a visão do mês ajuda a enxergar o
   todo.
7. **Marcar semana como doente ou viagem:** o treinador não conta como treino perdido, e a sequência
   fica protegida.
8. **Tamanho da fonte e contraste** ajustáveis (acessibilidade), e atalhos de teclado (Ctrl+I para
   importar, por exemplo).
9. **Tour de primeiro uso** de 4 passos, curto e pulável, apontando Hoje, Plano, Corridas e
   Conquistas.

## 4. Técnica

- **Tudo derivado dos dados:**
  - XP, estrelas, sequências e conquistas são calculados a partir das corridas e das sessões do
    plano (`src/coach/game.rs`, funções puras com testes).
  - "Recalcular métricas" recalcula tudo.
- **O que é guardado:** só o que não dá para derivar, numa tabela `game_seen`:
  - "já comemorado/visto", para a animação não repetir;
  - as semanas marcadas como doente ou viagem.
- **Animação:** desenhada pelo próprio egui (painter), sem dependências novas.
- **Notificações do Windows:** pela API do próprio sistema (`windows-sys`), sem rede.
- **Desligáveis em Configurações:** gamificação, animações e lembretes, cada um separado.

## 5. Ordem sugerida

| Fase | Entrega |
|---|---|
| 1 | Boletim pós-corrida (estrelas + XP), níveis, sequência semanal, tela de Conquistas (≈15 conquistas) |
| 2 | Mapa da jornada, missões da semana, comemorações |
| 3 | UX: arrastar e soltar, pasta vigiada, explicações "?", Hoje com ação principal, semana doente/viagem |
| 4 | Lembretes do Windows, calendário mensal, retrospectivas, tour e acessibilidade |

Cada fase sai como uma versão (0.3, 0.4…), com testes e capturas atualizadas.

## 6. Decisões (01/11/2026)

- **Tom:** discreto e adulto.
- **Versão 0.3:** fases 1, 2 e 3 juntas: boletim, XP, níveis, sequência, conquistas, mapa da jornada,
  missões da semana e melhorias de UX (arrastar e soltar, pasta vigiada, explicações "?" e semana
  doente/viagem).
- **Animações:** curtas (≈1,5 s), só em marcos importantes, e desligáveis.
- **Lembretes do Windows:** ficam para depois.
