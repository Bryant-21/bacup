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

    ObjectReference laserGrid = GetRef()
    if laserGrid == None || QuestScript == None
        return
    endif
    if OwningQuest.GetStage() >= LaserGridTurnedOffStage
        laserGrid.SetValue(QuestScript.W05_MQS_205P_LaserGridState, 1.0)
    else
        laserGrid.SetValue(QuestScript.W05_MQS_205P_LaserGridState, 0.0)
    endif
EndFunction
