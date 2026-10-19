# FitLedger — Plano (v0.2: treinador de corrida)

Em 19/10/2026 o app foi repensado: deixou de ser um diário multiesporte e virou um
**treinador de corrida** (rua e esteira) para quem está começando.

O objetivo do usuário é:
- **reduzir a FC** na corrida;
- e/ou bater uma meta de **distância ou tempo**.

---

## 1. Como funciona

1. O usuário define a **meta**: distância (obrigatória), tempo (opcional), FC média máxima (opcional,
   com o pace de referência), dias por semana, dias preferidos, dia do longão, superfície e data alvo
   (opcional).
2. O **treinador** gera o cronograma até a data alvo, ou até a data estimada.
3. A cada `.fit` importado (pasta `fits` ao lado do .exe, ou importação manual), ele:
   - liga a corrida à sessão planejada (mesmo dia, depois ±1 dia) e marca *feito*, *parcial* ou
     *não feito*;
   - reavalia o condicionamento: o modelo FC × velocidade das últimas 6 semanas, o volume, o maior
     percurso, o tempo previsto (Riegel) e a tendência da FC no pace da meta;
   - revisa as últimas 2 semanas: treinos perdidos, corridas leves com FC alta, deriva alta no
     longão, cansaço registrado (RPE, fadiga, dor);
   - **refaz o futuro**. O passado fica registrado, e cada ajuste aparece com a evidência no
     "Histórico de ajustes".
4. Mudar a meta cria uma **nova versão** dela. O plano é refeito a partir de hoje, e os treinos já
   feitos continuam no histórico.

### Regras do plano (conservadoras, para iniciante)

- **Fase de base:** só corridas leves (FC até o topo da zona 2) e longão, nas primeiras 3 semanas
  para quem tem pouco histórico.
- **Volume:** o tempo semanal cresce até 8% por semana (4% se as corridas leves estão fortes demais;
  0% se metade dos treinos foi perdida). A cada 4 semanas, uma semana mais leve (−20%).
- **Longão:** cresce cerca de 10% por semana até cerca de 105% da distância da meta.
- **Qualidade:** um treino de qualidade por semana depois da base. É intervalado no pace da meta
  quando há meta de tempo; ritmo moderado (zona 3) nos outros casos. Só entra se couber no volume.
- **Teste de evolução:** a cada 3 semanas, no pace da meta (de preferência na esteira). É o que mede
  a queda da FC.
- **Polimento:** −30% na última semana, com a tentativa da meta no dia do longão.
- **Zonas:** pela FC de reserva (Karvonen) por padrão. Sem FC máxima informada, o app usa
  208 − 0,7 × idade (Tanaka, 2001), ou 185 sem idade, e só considera a maior FC registrada
  quando ela for maior que isso.
- **Base científica:** a lista completa de métodos e estudos fica em `src/coach/science.rs`.
  Ela aparece no app (Plano → Base científica) e no README.

### Estimativa de quando a meta é viável

É o maior prazo entre três componentes:
- **distância:** semanas até o longão chegar à distância da meta;
- **tempo:** melhora de 0,8% por semana sobre o tempo previsto;
- **FC:** a tendência observada, ou 0,4 bpm por semana quando ainda não há tendência.

Se houver data alvo mais curta que a estimativa, aparece um aviso de "meta apertada" e o plano não
acelera além do seguro.

---

## 2. Telas

- **Hoje:** o treino do dia (ou o próximo), com pace, km/h da esteira, limite de FC e os blocos; o
  progresso da meta e a estimativa; a semana; as observações do treinador com os dados; a última
  corrida comparada ao plano; e o momento atual do corredor.
- **Plano:** as semanas com a fase, o tempo e os km; as sessões com status e detalhes; o histórico
  de ajustes.
- **Meta:** o formulário e o histórico de versões.
- **Corridas:** a lista com filtros e cada corrida em detalhe (gráficos, splits, voltas, zonas,
  deriva, percurso, notas com RPE, fadiga e dor, qualidade dos dados), além da comparação.
- **Análises:** evolução da meta, volume e aderência, pace × FC, deriva, records, zonas e o
  conversor.
- **Relatórios:** relatório HTML por período (imprimível em PDF) e CSV/JSON de corridas, plano e
  resumo semanal.
- **Configurações:** perfil e FC, zonas, idioma (automático, pt-BR ou en), tema (automático, claro
  ou escuro), pastas, backup e restauração, recálculo e atualizações.

## 3. Técnica (sem mudança desde a v0.1)

- Rust, egui (glow), SQLite portátil em `<exe>\data`, leitor FIT próprio, GPL-3.0.
- Esquema v2: `coach_goal` (versões da meta), `coach_session` (sessões e resultado) e `coach_log`
  (ajustes e evidência). As tabelas de tênis, musculação e planos manuais foram removidas, sempre
  com backup automático antes de migrar.
- Atualização automática: consulta o GitHub logo ao abrir e depois uma vez por dia; desligável.
- CI com fmt, clippy, testes e cargo-deny; release em rascunho com SHA-256 e atestado; Dependabot
  para actions e crates.

## 4. Futuro: trazer os treinos do Zepp sem copiar arquivos

Hoje o Zepp **não tem API pública para uso pessoal** (não encontrei documentação oficial). Os
caminhos possíveis, do mais simples ao mais trabalhoso:

1. **Pasta sincronizada (fácil, sem rede no app):** exportar o .fit pelo Zepp direto para uma pasta
   do Google Drive ou do OneDrive no celular e apontar o FitLedger para a pasta sincronizada no PC.
   Falta no app: escolher a pasta monitorada e vigiar mudanças com o app aberto.
2. **Intervals.icu (integração oficial com o Zepp desde jun/2025):** o Zepp envia os treinos para o
   Intervals.icu, que tem API pública com chave pessoal. O FitLedger poderia baixar as atividades
   novas.
   - Atenção: a documentação deles cita arquivos FIT e TCX, então pode ser preciso ler TCX também.
   - Seria a segunda conexão de rede do app, desligada por padrão.
3. **Strava:** o Zepp sincroniza com o Strava. A API do Strava (OAuth) entrega os dados por segundo
   (FC, distância, cadência, GPS, altitude), mas não o .fit original. Daria para criar a atividade a
   partir desses dados. Exige criar um "app" de API no Strava.
4. **Não recomendado:** APIs não oficiais da Huami/Zepp (exigem o token da conta e podem violar os
   termos de uso).

**Sugestão:** fazer primeiro o item 1 (pasta configurável e vigiada) e depois avaliar o item 2.

Fontes da pesquisa:
- https://forum.intervals.icu/t/amazfit-zepp-support-available/107652
- https://support.strava.com/hc/en-us/articles/216918437-Exporting-your-Data-and-GDPR
