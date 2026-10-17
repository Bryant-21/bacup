; Communing grants the blessing once per Equinox; refcol_PlayersAlreadyCommuned is cleared with the quest.
Event OnActivate(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If akActionRef != player
        Return
    EndIf
    If refcol_PlayersAlreadyCommuned != None
        If refcol_PlayersAlreadyCommuned.Find(player) >= 0
            Return
        EndIf
        refcol_PlayersAlreadyCommuned.AddRef(player)
    EndIf
    If crXPBonusSpell != None
        crXPBonusSpell.Cast(player, player)
    EndIf
    If E07A_Mothman_BlessingMessage != None
        E07A_Mothman_BlessingMessage.Show()
    EndIf
EndEvent
