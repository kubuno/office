// Augmentation LOCALE des types @kubuno/sdk : la session STT bas niveau
// (`startVoiceSession`) est exportée par le core mais le paquet npm publié est
// en retard (cf. autres shims « types npm en retard → adaptateur local »). À
// retirer au prochain bump de @kubuno/sdk qui inclut ces déclarations.
// `export {}` → fichier MODULE : `declare module` FUSIONNE avec les types
// existants du paquet (sinon il les REMPLACE et masque tous les autres exports).
export {}
declare module '@kubuno/sdk' {
  export type VoiceErrorCode = 'not-allowed' | 'audio-capture' | 'connect' | 'generic'
  export interface VoiceCallbacks {
    onReady?:   () => void
    onLevel?:   (level: number) => void
    onPartial?: (text: string) => void
    onResult?:  (text: string) => void
    onError?:   (code: VoiceErrorCode) => void
  }
  export interface VoiceSession { stop: () => void }
  export function startVoiceSession(lang: string, cb: VoiceCallbacks): Promise<VoiceSession>
}
