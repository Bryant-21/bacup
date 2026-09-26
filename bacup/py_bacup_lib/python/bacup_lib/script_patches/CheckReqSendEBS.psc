Event OnQuestInit()
    If EBSTopicToPlay == None
        Return
    EndIf
    If RequiredQuest != None && !RequiredQuest.IsCompleted()
        Return
    EndIf

    SQ_MasterScript masterScript = SQ_Master as SQ_MasterScript
    If masterScript == None
        Return
    EndIf
    Actor broadcaster = masterScript.EBSDefaultActor
    If broadcaster == None
        Return
    EndIf
    broadcaster.Say(EBSTopicToPlay)
EndEvent
