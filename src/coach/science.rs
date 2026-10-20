//! The evidence behind the coach, shown in the app ("Scientific basis").
//! Each item says what FitLedger does with it, and whether it comes from
//! research or is a conservative rule of thumb chosen by the app.

pub struct Basis {
    pub topic_pt: &'static str,
    pub topic_en: &'static str,
    pub use_pt: &'static str,
    pub use_en: &'static str,
    /// Empty for the app's own rules of thumb.
    pub reference: &'static str,
}

impl Basis {
    pub fn topic(&self) -> &'static str {
        if crate::i18n::pt() {
            self.topic_pt
        } else {
            self.topic_en
        }
    }

    pub fn usage(&self) -> &'static str {
        if crate::i18n::pt() { self.use_pt } else { self.use_en }
    }
}

pub const BASIS: &[Basis] = &[
    Basis {
        topic_pt: "Zonas pela frequência cardíaca de reserva (Karvonen)",
        topic_en: "Zones from heart-rate reserve (Karvonen)",
        use_pt: "As zonas são % da FC de reserva (máxima − repouso) somada à FC de repouso. O topo da zona 2 é o limite das corridas leves.",
        use_en: "Zones are a % of heart-rate reserve (max − rest) added to resting HR. The top of zone 2 is the cap of easy runs.",
        reference: "Karvonen MJ, Kentala E, Mustala O. The effects of training on heart rate: a longitudinal study. Ann Med Exp Biol Fenn. 1957;35(3):307–315.",
    },
    Basis {
        topic_pt: "FC máxima estimada pela idade",
        topic_en: "Age-predicted maximum heart rate",
        use_pt: "Sem FC máxima informada, usa 208 − 0,7 × idade (a maior FC registrada vale se for maior).",
        use_en: "Without a known max HR, uses 208 − 0.7 × age (the highest recorded HR counts if it is higher).",
        reference: "Tanaka H, Monahan KD, Seals DR. Age-predicted maximal heart rate revisited. J Am Coll Cardiol. 2001;37(1):153–156.",
    },
    Basis {
        topic_pt: "Maior parte do treino em baixa intensidade",
        topic_en: "Most training at low intensity",
        use_pt: "A base é só de corridas leves e, depois, no máximo um treino mais forte por semana: o resto continua leve.",
        use_en: "The base phase is easy running only, and afterwards at most one harder session a week: the rest stays easy.",
        reference: "Seiler KS, Kjerland GØ. Quantifying training intensity distribution in elite endurance athletes: is there evidence for an \"optimal\" distribution? Scand J Med Sci Sports. 2006;16(1):49–56.",
    },
    Basis {
        topic_pt: "FC submáxima como medida de evolução",
        topic_en: "Submaximal heart rate as a progress marker",
        use_pt: "Com o treino, a FC no mesmo ritmo cai. O app mede isso nos testes de evolução (mesmo pace, de preferência na esteira) e em todas as corridas, ajustadas pela relação FC × velocidade.",
        use_en: "With training, HR at the same pace drops. The app measures it in progress checks (same pace, ideally on the treadmill) and in every run, adjusted by the HR × speed relation.",
        reference: "Buchheit M. Monitoring training status with HR measures: do all roads lead to Rome? Front Physiol. 2014;5:73.",
    },
    Basis {
        topic_pt: "Deriva cardiovascular",
        topic_en: "Cardiovascular drift",
        use_pt: "Compara FC e ritmo entre as metades da corrida (desacoplamento). Deriva alta no longão segura o aumento do longão.",
        use_en: "Compares HR and pace between the halves of a run (decoupling). High drift on the long run holds back its growth.",
        reference: "Coyle EF, González-Alonso J. Cardiovascular drift during prolonged exercise: new perspectives. Exerc Sport Sci Rev. 2001;29(2):88–92.",
    },
    Basis {
        topic_pt: "Carga de treino (TRIMP)",
        topic_en: "Training load (TRIMP)",
        use_pt: "A carga de cada corrida combina duração e intensidade pela FC, com os coeficientes de Banister para homens ou mulheres (o sexo é opcional e só afeta a carga).",
        use_en: "Each run's load combines duration and heart-rate intensity, with Banister's coefficients for men or women (sex is optional and only affects load).",
        reference: "Banister EW. Modeling elite athletic performance. In: MacDougall JD, Wenger HA, Green HJ, eds. Physiological Testing of the High-Performance Athlete. Human Kinetics; 1991:403–424.",
    },
    Basis {
        topic_pt: "Previsão de tempo para outra distância",
        topic_en: "Predicting time over another distance",
        use_pt: "Tempo previsto = tempo conhecido × (distância da meta ÷ distância conhecida)^1,06.",
        use_en: "Predicted time = known time × (goal distance ÷ known distance)^1.06.",
        reference: "Riegel PS. Athletic records and human endurance. Am Sci. 1981;69(3):285–290.",
    },
    Basis {
        topic_pt: "Progressão gradual do volume",
        topic_en: "Gradual volume progression",
        use_pt: "Iniciantes que aumentam muito a distância semanal se lesionam mais. O app aumenta no máximo 8% por semana. Esse número exato é uma escolha conservadora: um estudo não mostrou que limitar a 10% reduz lesões em relação a um programa padrão.",
        use_en: "Novice runners who increase weekly distance a lot get injured more. The app grows at most 8% a week. That exact number is a conservative choice: one study found that capping at 10% did not reduce injuries compared with a standard program.",
        reference: "Nielsen RØ et al. Excessive progression in weekly running distance and risk of running-related injuries. J Orthop Sports Phys Ther. 2014;44(10):739–747. · Buist I et al. No effect of a graded training program on the number of running-related injuries in novice runners: a randomized controlled trial. Am J Sports Med. 2008;36(1):33–39.",
    },
    Basis {
        topic_pt: "Periodização e semanas de recuperação",
        topic_en: "Periodization and recovery weeks",
        use_pt: "Fases (base, desenvolvimento, polimento) e uma semana mais leve a cada quatro.",
        use_en: "Phases (base, build, taper) and a lighter week every fourth week.",
        reference: "Issurin VB. New horizons for the methodology and physiology of training periodization. Sports Med. 2010;40(3):189–206.",
    },
    Basis {
        topic_pt: "Regras próprias do FitLedger (não vêm de um estudo)",
        topic_en: "FitLedger's own rules (not from a study)",
        use_pt: "Longão +10% por semana; melhora estimada de 0,8% por semana no tempo e de 0,4 bpm por semana na FC quando ainda não há tendência; 3 semanas de base para quem tem pouco histórico; deriva acima de 8% e mais de 30% do tempo acima da zona 2 como sinais de alerta. São escolhas conservadoras, refeitas com os seus dados a cada corrida.",
        use_en: "Long run +10% a week; expected improvement of 0.8% a week in time and 0.4 bpm a week in HR when there is no trend yet; 3 base weeks with little history; drift above 8% and more than 30% of the time above zone 2 as warning signs. These are conservative choices, redone with your data after every run.",
        reference: "",
    },
];
