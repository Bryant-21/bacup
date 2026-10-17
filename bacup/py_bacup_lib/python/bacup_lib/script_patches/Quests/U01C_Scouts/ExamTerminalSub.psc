Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    Quests:U01C_Scouts:ExamTerminalMain examTerminal = akTerminalRef as Quests:U01C_Scouts:ExamTerminalMain
    If examTerminal == None
        Return
    EndIf
    Quests:U01C_Scouts:ExamTerminalQuest examQuest = examTerminal.P01C_BadgeTerminal_Quest as Quests:U01C_Scouts:ExamTerminalQuest
    If examQuest != None
        examQuest.AnswerQuestion(CorrectAnswers != None && CorrectAnswers.Find(auiMenuItemID) >= 0)
    EndIf
EndEvent
