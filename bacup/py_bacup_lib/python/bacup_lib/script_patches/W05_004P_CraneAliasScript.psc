Event OnAliasInit()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None
        RegisterForRemoteEvent(owningQuest, "OnStageSet")
    EndIf
    RegisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
    RegisterForCraneHit()
EndEvent

Event OnLoad()
    RegisterForCraneHit()
EndEvent

Event Actor.OnPlayerLoadGame(Actor akSender)
    If akSender == Game.GetPlayer()
        RegisterForCraneHit()
    EndIf
EndEvent

Event Quest.OnStageSet(Quest akSender, Int auiStageID, Int auiItemID)
    If akSender == GetOwningQuest()
        RegisterForCraneHit()
    EndIf
EndEvent

Event OnAliasShutdown()
    UnregisterForAllHitEvents()
    UnregisterForRemoteEvent(GetOwningQuest(), "OnStageSet")
    UnregisterForRemoteEvent(Game.GetPlayer(), "OnPlayerLoadGame")
EndEvent

Function RegisterForCraneHit()
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsRunning() && owningQuest.GetStage() >= RegistrationStage && StagetoSetOnHit > 0 && !owningQuest.IsStageDone(StagetoSetOnHit) && GetReference() != None && OwningPlayer != None && OwningPlayer.GetReference() != None
        RegisterForHitEvent(Self, OwningPlayer)
    EndIf
EndFunction

Event OnHit(ObjectReference akTarget, ObjectReference akAggressor, Form akSource, Projectile akProjectile, Bool abPowerAttack, Bool abSneakAttack, Bool abBashAttack, Bool abHitBlocked, String apMaterial)
    Quest owningQuest = GetOwningQuest()
    If owningQuest != None && owningQuest.IsRunning() && owningQuest.GetStage() >= RegistrationStage && StagetoSetOnHit > 0 && !owningQuest.IsStageDone(StagetoSetOnHit) && akTarget != None && akTarget == GetReference() && OwningPlayer != None && akAggressor != None && akAggressor == OwningPlayer.GetReference()
        owningQuest.SetStage(StagetoSetOnHit)
    EndIf
    RegisterForCraneHit()
EndEvent

Event OnDeath(Actor akKiller)
    Actor playerRef = OwningPlayer.GetActorReference()
    If playerRef == None
        playerRef = Game.GetPlayer()
    EndIf
    Actor solRef = Sol.GetActorReference()
    If playerRef != None && solRef != None && akKiller == solRef && playerRef.GetValue(W05_MQ_004P_Crane_SolKilledCrane) < 1.0
        playerRef.SetValue(W05_MQ_004P_Crane_SolKilledCrane, 1.0)
    EndIf
EndEvent
