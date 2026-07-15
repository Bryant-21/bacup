Event OnActivate(ObjectReference akActionRef)
    Actor activatingPlayer = akActionRef as Actor

    If activatingPlayer == None || activatingPlayer != Game.GetPlayer()
        Return
    EndIf

    ; The default-state event is also used by the skeleton's named states. This
    ; guard makes activation re-entrant safe while a dispense is in progress.
    If interactingPlayer != None
        Return
    EndIf

    If activatingPlayer.HasMagicEffectWithKeyword(CooldownKeyword)
        RechargingMessage.Show()
        Return
    EndIf

    If activatingPlayer.GetItemCount(Caps001) < RequiredCaps
        NoCapsMessage.Show()
        Return
    EndIf

    interactingPlayer = activatingPlayer
    GoToState("dispensebusy")
    BlockActivation(True, False)

    activatingPlayer.RemoveItem(Caps001, RequiredCaps, True)
    activatingPlayer.AddItem(pColaDispensed, 1, False)
    CooldownSpell.Cast(activatingPlayer, activatingPlayer)
    PlayAnimation("Play01")

    interactingPlayer = None
    BlockActivation(False, False)
    GoToState("DispenseStopped")
EndEvent
