Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 5
        Quest targetQuest = masterScript.MoMQuestList[5].MoMQuest
        Int stage = masterScript.CONST_MoM02B_FindThePassword
        If targetQuest != None && !targetQuest.IsStageDone(stage)
            targetQuest.SetStage(stage)
        EndIf
    EndIf
EndFunction
