Event OnInit()
    CurrentQuestionIndex = -1
EndEvent

Bool Function IsExamInProgress(Quests:U01C_Scouts:ExamTerminalMain:Exam akExam)
    Return akExam != None && CurrentExam == akExam && CurrentQuestionIndex >= 0
EndFunction

Function StartExam(ObjectReference akTerminalRef, Quests:U01C_Scouts:ExamTerminalMain:Exam akExam)
    If akExam == None
        Return
    EndIf
    If !IsRunning()
        Start()
    EndIf
    PlayerRef = Game.GetPlayer()
    TerminalRef = akTerminalRef
    CurrentExam = akExam
    If Alias_BadgeTerminal != None && akTerminalRef != None
        Alias_BadgeTerminal.ForceRefTo(akTerminalRef)
    EndIf
    Score = 0
    NumQuestionsAnswered = 0
    Passed = False
    Int poolSize = akExam.NumQuestionsMax
    If poolSize < 1
        poolSize = 1
    EndIf
    AvailableQuestionIndexes = new Int[poolSize]
    Int index = 0
    While index < poolSize
        AvailableQuestionIndexes[index] = index
        index += 1
    EndWhile
    CurrentQuestionIndex = DrawQuestion()
EndFunction

; Questions are drawn without repeats from the exam's pool; -1 ends the exam
; and switches the question menu to "View Score".
Int Function DrawQuestion()
    If CurrentExam == None || NumQuestionsAnswered >= CurrentExam.NumQuestions || AvailableQuestionIndexes == None || AvailableQuestionIndexes.Length == 0
        Return -1
    EndIf
    Int slot = Utility.RandomInt(0, AvailableQuestionIndexes.Length - 1)
    Int question = AvailableQuestionIndexes[slot]
    AvailableQuestionIndexes.Remove(slot)
    Return question
EndFunction

Function AnswerQuestion(Bool abCorrect)
    If CurrentExam == None || CurrentQuestionIndex < 0
        Return
    EndIf
    NumQuestionsAnswered += 1
    If abCorrect
        Score += 1
    EndIf
    CurrentQuestionIndex = DrawQuestion()
    If CurrentQuestionIndex < 0
        FinishExam()
    EndIf
EndFunction

; Passing needs every question right. FO76 completed the exam's sub-challenge
; here; Tales' challenge engine matches that sub-challenge on the exam's start
; subterminal, and the badge parent then grants the Tadpole/Possum badge.
Function FinishExam()
    Passed = CurrentExam != None && Score >= CurrentExam.NumQuestions
    If Passed && CurrentExam.StartTerminal != None
        Var[] args = new Var[1]
        args[0] = CurrentExam.StartTerminal
        Utility.CallGlobalFunction("B21:B21_TFA_ChallengesNative", "Scripted", args)
    EndIf
EndFunction
