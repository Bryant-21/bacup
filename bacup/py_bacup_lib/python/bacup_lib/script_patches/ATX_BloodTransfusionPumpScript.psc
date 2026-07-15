; Method fill for the partially stripped FO76 blood transfusion pump.
; The generated skeleton keeps the declarations and named states. Moving to the
; default state lets this top-level activation handler replace the incomplete
; Ready-state behavior without redistributing the original script.

Event OnLoad()
    BlockActivation(False, False)
    GoToState("")
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer()
        Return
    EndIf

    interactingPlayer = akActionRef as Actor
    If interactingPlayer == None
        Return
    EndIf

    If interactingPlayer.HasMagicEffectWithKeyword(CooldownKeyword)
        RechargingMessage.Show()
        Return
    EndIf

    BlockActivation(True, False)
    SoundID = ActivateSound.Play(Self)
    BuffSpell.Cast(Self, interactingPlayer)
    CooldownSpell.Cast(Self, interactingPlayer)
    BlockActivation(False, False)
EndEvent
