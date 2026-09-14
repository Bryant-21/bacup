Function Fragment_Terminal_26(ObjectReference akTerminalRef)
    MoMMasterQuestScript masterScript = MoMMaster as MoMMasterQuestScript
    If masterScript != None && masterScript.MoMQuestList.Length > 8
        Actor player = Game.GetPlayer()
        player.SetValue(MoMHeadmistressOfficeAccessValue, 1.0)
        Quest targetQuest = masterScript.MoMQuestList[8].MoMQuest
        Int stage = masterScript.CONST_MoM04_GainedAccessToHeadmistressOffice
        If targetQuest != None && !targetQuest.IsStageDone(stage)
            targetQuest.SetStage(stage)
        EndIf
    EndIf
EndFunction
