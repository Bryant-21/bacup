Function SetMoM03Stage(Int aiStage)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 7
        Quest targetQuest = masterScript.MoMQuestList[7].MoMQuest
        If targetQuest != None && !targetQuest.IsStageDone(aiStage)
            targetQuest.SetStage(aiStage)
        EndIf
    EndIf
EndFunction

Function Fragment_Terminal_03(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        SetMoM03Stage(masterScript.CONST_MoM03_DispensedHolotape)
    EndIf
EndFunction

Function Fragment_Terminal_04(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        SetMoM03Stage(masterScript.CONST_MoM03_MountedHolotapeDrive)
    EndIf
EndFunction

Function Fragment_Terminal_05(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None
        SetMoM03Stage(masterScript.CONST_MoM03_ViewedReadme)
    EndIf
EndFunction
