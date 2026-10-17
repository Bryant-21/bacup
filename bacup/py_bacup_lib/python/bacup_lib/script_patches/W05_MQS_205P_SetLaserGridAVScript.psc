Event OnAliasInit()
    OwningQuest = GetOwningQuest()
    QuestScript = OwningQuest as W05_MQS_205_Script
    if OwningQuest != None
        RegisterForRemoteEvent(OwningQuest, "OnStageSet")
    endif
    RefreshLaserGridState()
EndEvent

Event OnLoad()
    RefreshLaserGridState()
EndEvent

Event Quest.OnStageSet(Quest akSender, int auiStageID, int auiItemID)
    RefreshLaserGridState()
EndEvent

Function RefreshLaserGridState()
    if OwningQuest == None
        OwningQuest = GetOwningQuest()
    endif
    if QuestScript == None
        QuestScript = OwningQuest as W05_MQS_205_Script
    endif

    ; This alias is the EnterBasementTrigger; the grids (55F73F/55F744/55F747) read the AV from the player.
    Actor playerRef = Game.GetPlayer()
    if QuestScript == None
        return
    endif
    if OwningQuest.GetStage() >= LaserGridTurnedOffStage
        playerRef.SetValue(QuestScript.W05_MQS_205P_LaserGridState, 1.0)
    else
        playerRef.SetValue(QuestScript.W05_MQS_205P_LaserGridState, 0.0)
    endif
EndFunction

Event OnTriggerEnter(ObjectReference akActionRef)
    if akActionRef == Game.GetPlayer()
        RefreshLaserGridState()
    endif
EndEvent
