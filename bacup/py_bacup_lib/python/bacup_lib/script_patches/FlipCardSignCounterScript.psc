; FO76 replicated count changes through OnSyncVariableNetworkChanged; OnCellLoad and
; direct DisplayCount calls cover single player.
; @drop-member OnSyncVariableNetworkChanged

Int Function DigitAtPlace(Int aiValue, Int aiPlace, Int aiMissingDigit)
    Int divisor = 1
    Int step = 1
    While step < aiPlace
        divisor = divisor * 10
        step += 1
    EndWhile
    If aiValue < divisor
        Return aiMissingDigit
    EndIf
    Return (aiValue / divisor) % 10
EndFunction

String Function GetEventString(String Character, Int Position)
    If Position == IndexOfHundredsPlace
        Character = DigitAtPlace(GetCount(), 3, -1) as String
        If Character == "-1"
            Character = LeadingZeroCharacter
        EndIf
    ElseIf Position == IndexOfTensPlace
        Character = DigitAtPlace(GetCount(), 2, -1) as String
        If Character == "-1"
            Character = LeadingZeroCharacter
        EndIf
    ElseIf Position == IndexOfOnesPlace
        Character = DigitAtPlace(GetCount(), 1, 0) as String
    EndIf
    Return parent.GetEventString(Character, Position)
EndFunction

Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity, String DejaSubChannel, Bool bShowNormalTrace)
    Debug.OpenUserLog("FlipCardSign")
    Return Debug.TraceUser("FlipCardSign", CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction
