Event OnAliasInit()
    RegisterForGangerHit()
EndEvent

Event OnLoad()
    RegisterForGangerHit()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        RegisterForGangerHit()
    EndIf
EndEvent

Event OnAliasShutdown()
    UnregisterForAllHitEvents()
    UnregisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
EndEvent

Function RegisterForGangerHit()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsRunning() && StageToSet >= 0 && !owningQuest.IsStageDone(StageToSet) && GetReference() != None && OwningPlayer != None && OwningPlayer.GetReference() != None
        RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
        RegisterForHitEvent(Self, OwningPlayer)
    EndIf
EndFunction

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, bool abPowerAttack, bool abSneakAttack, bool abBashAttack, bool abHitBlocked, string apMaterial)
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsRunning() && StageToSet >= 0 && !owningQuest.IsStageDone(StageToSet) && akTarget != None && akTarget == GetReference() && OwningPlayer != None && akAggressor != None && akAggressor == OwningPlayer.GetReference()
        owningQuest.SetStage(StageToSet)
    EndIf
    RegisterForGangerHit()
EndEvent
