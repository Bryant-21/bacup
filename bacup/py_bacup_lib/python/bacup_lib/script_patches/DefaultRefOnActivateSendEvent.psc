Event OnActivate(ObjectReference akActionRef)
    Actor player = Game.GetPlayer()
    If PlayerTriggerOnly && akActionRef != player
        Return
    EndIf
    If akActionRef == player
        If BlockWhilePlayerIsSitting && player.GetSitState() != 0
            Return
        EndIf
        If BlockWhilePlayerIsInPowerArmor && player.IsInPowerArmor()
            Return
        EndIf
        If BlockWhilePlayerIsInCombat && player.IsInCombat()
            Return
        EndIf
    EndIf
    SendConfiguredStoryEvent()
EndEvent
