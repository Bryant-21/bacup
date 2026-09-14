Function Fragment_Terminal_01(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && MoM00 != None
        Int stage = masterScript.CONST_MoM00_ReadStudyTerminal
        If !MoM00.IsStageDone(stage)
            MoM00.SetStage(stage)
        EndIf
    EndIf
EndFunction

Function Fragment_Terminal_11(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 8
        Quest targetQuest = masterScript.MoMQuestList[8].MoMQuest
        Int stage = masterScript.CONST_MoM04_FoundStudyClue
        If targetQuest != None && !targetQuest.IsStageDone(stage)
            targetQuest.SetStage(stage)
        EndIf
    EndIf
EndFunction
