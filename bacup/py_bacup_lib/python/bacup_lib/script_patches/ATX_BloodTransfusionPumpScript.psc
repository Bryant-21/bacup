Event OnLoad()
    BlockActivation(False, False)
    GoToState("Ready")
EndEvent

State Ready
    Event OnActivate(ObjectReference akActionRef)
        TryTransfusion(akActionRef)
    EndEvent
EndState

State processing
    Event OnActivate(ObjectReference akActionRef)
    EndEvent
EndState

Event OnActivate(ObjectReference akActionRef)
    TryTransfusion(akActionRef)
EndEvent

Function TryTransfusion(ObjectReference akActionRef)
    If akActionRef != Game.GetPlayer() || GetState() == "processing"
        Return
    EndIf
    GoToState("processing")
    interactingPlayer = akActionRef as Actor
    If interactingPlayer.HasMagicEffectWithKeyword(CooldownKeyword)
        RechargingMessage.Show()
        GoToState("Ready")
        Return
    EndIf

    SoundID = ActivateSound.Play(Self)
    BuffSpell.Cast(interactingPlayer, interactingPlayer)
    CooldownSpell.Cast(interactingPlayer, interactingPlayer)
    GoToState("Ready")
EndFunction
