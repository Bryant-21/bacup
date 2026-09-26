Event OnDying(Actor akKiller)
    HandleBossDefeated()
EndEvent

Event OnDeath(Actor akKiller)
    HandleBossDefeated()
EndEvent

Function HandleBossDefeated()
    Quest owningQuest = GetOwningQuest()
    Actor boss = GetActorReference()
    If owningQuest == None || boss == None || !owningQuest.IsRunning()
        Return
    EndIf
    Quests:Storm:RegionBoss:RegionBossQuestScript bossEvent = owningQuest as Quests:Storm:RegionBoss:RegionBossQuestScript
    If bossEvent != None && bossEvent.EventResolved()
        Return
    EndIf
    If iObjectiveToCompleteOnDeath >= 0 && !owningQuest.IsObjectiveCompleted(iObjectiveToCompleteOnDeath)
        owningQuest.SetObjectiveCompleted(iObjectiveToCompleteOnDeath, True)
    EndIf
    If bossEvent != None
        bossEvent.HandleBossDeath(boss)
    EndIf
EndFunction

Function SelfDestruct()
    Actor boss = GetActorReference()
    If boss == None || boss.IsDead() || RobotSelfDestructSpell == None
        Return
    EndIf
    boss.AddSpell(RobotSelfDestructSpell, False)
EndFunction
