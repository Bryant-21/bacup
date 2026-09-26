; [Misc Pointer to Big Bend Tunnel]. FO76 handed this to players who were told about the caravan so
; they could travel to it; the quest carries DefaultQuestSetStageOnTimerScript (120 s -> 9999) and a
; distance check on the entrance marker (-> 9000). It is event scoped (ENAM = SCPT) and no converted
; Story Manager node raises its keyword, so these fragments only run if that node is restored.
Function Fragment_Stage_0100_Item_00()
    SetObjectiveDisplayed(10, True, True)
EndFunction

Function Fragment_Stage_9000_Item_00()
    If IsObjectiveDisplayed(10) && !IsObjectiveCompleted(10)
        SetObjectiveCompleted(10, True)
    EndIf
    Stop()
EndFunction

Function Fragment_Stage_9999_Item_00()
    If IsObjectiveDisplayed(10) && !IsObjectiveCompleted(10)
        SetObjectiveFailed(10, True)
    EndIf
    Stop()
EndFunction
