Function RegisterExamMenus()
    If P01C_BadgeTerminal_Tadpole != None
        RegisterForRemoteEvent(P01C_BadgeTerminal_Tadpole, "OnMenuItemRun")
    EndIf
    If P01C_BadgeTerminal_Possum != None
        RegisterForRemoteEvent(P01C_BadgeTerminal_Possum, "OnMenuItemRun")
    EndIf
    Int index = 0
    While AllExams != None && index < AllExams.Length
        If AllExams[index] != None && AllExams[index].StartTerminal != None
            RegisterForRemoteEvent(AllExams[index].StartTerminal, "OnMenuItemRun")
        EndIf
        index += 1
    EndWhile
EndFunction

Event OnInit()
    RegisterExamMenus()
EndEvent

Event OnLoad()
    RegisterExamMenus()
EndEvent

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == Game.GetPlayer()
        RegisterExamMenus()
    EndIf
EndEvent

; Picking an exam on the Tadpole/Possum menu prepares it before its start
; subterminal opens, so the question menu's CurrentQuestionIndex conditions
; already hold a question when "Start Exam" shows it. Start Exam itself only
; prepares when no attempt at that exam is in progress.
Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
    If akTerminalRef != Self || AllExams == None
        Return
    EndIf
    Quests:U01C_Scouts:ExamTerminalQuest examQuest = P01C_BadgeTerminal_Quest as Quests:U01C_Scouts:ExamTerminalQuest
    If examQuest == None
        Return
    EndIf
    Int index = -1
    If akSender == P01C_BadgeTerminal_Tadpole || akSender == P01C_BadgeTerminal_Possum
        index = AllExams.FindStruct("MenuID", auiMenuItemID)
        If index >= 0 && AllExams[index].StartTerminal != None
            examQuest.StartExam(Self, AllExams[index])
        EndIf
    ElseIf auiMenuItemID == 1
        index = AllExams.FindStruct("StartTerminal", akSender)
        If index >= 0 && !examQuest.IsExamInProgress(AllExams[index])
            examQuest.StartExam(Self, AllExams[index])
        EndIf
    EndIf
EndEvent
