Function Fragment_End()
    W05_MQ_004P_Crane_QuestScript owningQuest = GetOwningQuest() as W05_MQ_004P_Crane_QuestScript
    ; The "Kill 'em" refusals set 1220 with EndRunningScene, skipping the phases that set 1230.
    If owningQuest != None && owningQuest.IsRunning() && owningQuest.IsStageDone(1220) && !owningQuest.IsStageDone(1230) && !owningQuest.IsStageDone(1250)
        owningQuest.SetStage(1230)
        Return
    EndIf
    If owningQuest != None && owningQuest.IsRunning() && owningQuest.IsStageDone(1260) && !owningQuest.IsStageDone(1220) && !owningQuest.IsStageDone(1221) && !owningQuest.IsStageDone(1230)
        If !owningQuest.IsStageDone(1300)
            owningQuest.SetStage(1300)
        EndIf
        owningQuest.EvaluateRadicalWalkOut()
    EndIf
EndFunction
