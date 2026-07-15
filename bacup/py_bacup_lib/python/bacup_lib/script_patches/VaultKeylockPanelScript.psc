Function ProcessKeylockActivation(ObjectReference akActionRef)
    If lock_ProcessActivation
        Return
    EndIf

    Actor activatingPlayer = akActionRef as Actor
    If activatingPlayer == None || activatingPlayer != Game.GetPlayer()
        Return
    EndIf

    lock_ProcessActivation = True
    If KeylockPanelKey != None && activatingPlayer.GetItemCount(KeylockPanelKey) > 0
        SetLocalOpen(True, True)
    Else
        KeylockPanel_PlayFailureSound()
        KeylockPanelMessageRequiresKey.Show()
    EndIf
    lock_ProcessActivation = False
EndFunction

Auto State StartsClosed
    Event OnActivate(ObjectReference akActionRef)
        ProcessKeylockActivation(akActionRef)
    EndEvent
EndState

State closed
    Event OnActivate(ObjectReference akActionRef)
        ProcessKeylockActivation(akActionRef)
    EndEvent
EndState
