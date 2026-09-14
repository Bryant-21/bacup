Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 2
        Quest targetQuest = masterScript.MoMQuestList[2].MoMQuest
        Int stage = masterScript.CONST_MoM01_AuthorizedPromotion
        If targetQuest != None && !targetQuest.IsStageDone(stage)
            targetQuest.SetStage(stage)
        EndIf
    EndIf
EndFunction
