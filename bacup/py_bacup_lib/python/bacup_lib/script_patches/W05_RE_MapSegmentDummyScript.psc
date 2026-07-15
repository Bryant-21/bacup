Function ShowNumber(Int numberToShow)
    NumberPosition = GetLinkedRef(W05_RE_NumberPosition_Keyword)
    MapSegment = GetLinkedRef(W05_RE_MapSegment_Keyword)
    If NumberPosition == None || MapSegment == None || !MapSegment.IsEnabled()
        ClearNumber()
        Return
    EndIf

    Static numberStatic = GetNumberStatic(numberToShow)
    If numberStatic == None
        Return
    EndIf

    ClearNumber()
    CurrentlyPlacedNumber = NumberPosition.PlaceAtMe(numberStatic)
EndFunction

Function ClearNumber()
    If CurrentlyPlacedNumber != None
        CurrentlyPlacedNumber.Disable()
        CurrentlyPlacedNumber.Delete()
        CurrentlyPlacedNumber = None
    EndIf
EndFunction

Static Function GetNumberStatic(Int numberToShow)
    If numberToShow == 0
        Return ChalkLetter_Math_0
    ElseIf numberToShow == 1
        Return ChalkLetter_Math_1
    ElseIf numberToShow == 2
        Return ChalkLetter_Math_2
    ElseIf numberToShow == 3
        Return ChalkLetter_Math_3
    ElseIf numberToShow == 4
        Return ChalkLetter_Math_4
    ElseIf numberToShow == 5
        Return ChalkLetter_Math_5
    ElseIf numberToShow == 6
        Return ChalkLetter_Math_6
    ElseIf numberToShow == 7
        Return ChalkLetter_Math_7
    ElseIf numberToShow == 8
        Return ChalkLetter_Math_8
    ElseIf numberToShow == 9
        Return ChalkLetter_Math_9
    EndIf

    Return None
EndFunction
