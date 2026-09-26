Function RecordAnswer(Terminal akQuestionTerminal, Int aiResponseValue)
    If akQuestionTerminal == None
        Return
    EndIf
    If QuestionTerminals == None
        QuestionTerminals = new TerminalDatum[0]
    EndIf
    Int i = 0
    While i < QuestionTerminals.Length
        If QuestionTerminals[i].TargetTerminal == akQuestionTerminal
            TerminalDatum updatedAnswer = QuestionTerminals[i]
            updatedAnswer.iResponseValue = aiResponseValue
            QuestionTerminals[i] = updatedAnswer
            RecountCorrectAnswers()
            Return
        EndIf
        i = i + 1
    EndWhile
    TerminalDatum newAnswer = new TerminalDatum
    newAnswer.TargetTerminal = akQuestionTerminal
    newAnswer.iResponseValue = aiResponseValue
    QuestionTerminals.Add(newAnswer)
    RecountCorrectAnswers()
EndFunction

Function ResetAnswers()
    Int i = 0
    While QuestionTerminals != None && i < QuestionTerminals.Length
        TerminalDatum clearedAnswer = QuestionTerminals[i]
        clearedAnswer.iResponseValue = 0
        QuestionTerminals[i] = clearedAnswer
        i = i + 1
    EndWhile
    iPlayerCorrectAnswers = 0
EndFunction

Function RecountCorrectAnswers()
    Int total = 0
    Int i = 0
    While QuestionTerminals != None && i < QuestionTerminals.Length
        If QuestionTerminals[i].iResponseValue > 0
            total = total + QuestionTerminals[i].iResponseValue
        EndIf
        i = i + 1
    EndWhile
    iPlayerCorrectAnswers = total
EndFunction
