Event OnActivate(ObjectReference akActionRef)
    Actor activatingPlayer = akActionRef as Actor
    If activatingPlayer != Game.GetPlayer()
        Return
    EndIf

    If activatingPlayer.GetValue(W05_PlayerCanAccessVault79Entrance) <= 0.0
        Return
    EndIf

    If thisLaserGrid != None
        thisLaserGrid.Disable(False)
    EndIf
    If thisDoorToOpen != None
        thisDoorToOpen.SetOpen(True)
    EndIf
EndEvent
