; FO76 Debug.TraceLog is unavailable in FO4. Preserve the Bool contract with
; FO4's user-log API.

Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity, String DejaSubChannel, Bool bShowNormalTrace)
    Debug.OpenUserLog("FlipCardSign")
    Return Debug.TraceUser("FlipCardSign", CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction

Function StartMessageTimer(Int IndexVariable)
    If IndexVariable == 0 && DisplayTimeMessage0 > 0.0
        StartTimer(DisplayTimeMessage0, 0)
    ElseIf IndexVariable == 1 && DisplayTimeMessage1 > 0.0
        StartTimer(DisplayTimeMessage1, 0)
    ElseIf IndexVariable == 2 && DisplayTimeMessage2 > 0.0
        StartTimer(DisplayTimeMessage2, 0)
    ElseIf IndexVariable == 3 && DisplayTimeMessage3 > 0.0
        StartTimer(DisplayTimeMessage3, 0)
    ElseIf IndexVariable == 4 && DisplayTimeMessage4 > 0.0
        StartTimer(DisplayTimeMessage4, 0)
    ElseIf IndexVariable == 5 && DisplayTimeMessage5 > 0.0
        StartTimer(DisplayTimeMessage5, 0)
    ElseIf IndexVariable == 6 && DisplayTimeMessage6 > 0.0
        StartTimer(DisplayTimeMessage6, 0)
    ElseIf IndexVariable == 7 && DisplayTimeMessage7 > 0.0
        StartTimer(DisplayTimeMessage7, 0)
    ElseIf IndexVariable == 8 && DisplayTimeMessage8 > 0.0
        StartTimer(DisplayTimeMessage8, 0)
    ElseIf IndexVariable == 9 && DisplayTimeMessage9 > 0.0
        StartTimer(DisplayTimeMessage9, 0)
    ElseIf IndexVariable == 10 && DisplayTimeMessage10 > 0.0
        StartTimer(DisplayTimeMessage10, 0)
    EndIf
EndFunction

Function SetMessageIndex()
    If CountMessages > 0
        ClientOnlyMessageIndex = (ClientOnlyMessageIndex + 1) % CountMessages
    Else
        ClientOnlyMessageIndex = 0
    EndIf
EndFunction

String Function GetEventString(String Character, Int Position)
    Int index
    If CharEventMapData == None
        Return ""
    EndIf

    index = CharEventMapData.FindStruct("Character", Character)
    If index < 0
        Return ""
    EndIf
    Return CharEventMapData[index].AnimationEvent
EndFunction

Function DisplayMessage(String[] messageTextToDisplay)
    Int I
    ObjectReference thisRef
    String eventString
    String blankAnim

    While DisplayMessageSpinLock
        Utility.Wait(0.1)
    EndWhile
    DisplayMessageSpinLock = True

    If messageTextToDisplay == None
        messageTextToDisplay = GetMessageToDisplay(ClientOnlyMessageIndex)
    EndIf
    If messageTextToDisplay == None || LinkedRefs == None
        DisplayMessageSpinLock = False
        Return
    EndIf

    I = 0
    While I < messageTextToDisplay.Length && I < LinkedRefs.Length
        thisRef = LinkedRefs[I]
        eventString = GetEventString(messageTextToDisplay[I], I)
        If thisRef != None && eventString != "" && thisRef.Is3DLoaded()
            thisRef.PlayAnimation(eventString)
        EndIf
        I += 1
    EndWhile

    If ClearEndOfMessage
        blankAnim = GetEventString(" ", 0)
        While I < LinkedRefs.Length
            thisRef = LinkedRefs[I]
            If thisRef != None && blankAnim != "" && thisRef.Is3DLoaded()
                thisRef.PlayAnimation(blankAnim)
            EndIf
            I += 1
        EndWhile
    EndIf
    DisplayMessageSpinLock = False
EndFunction

Event OnLoad()
    DisplayMessageSpinLock = False
    SetCountMessages()
    LinkedRefs = GetLinkedRefChain(None, 100)
    DisplayMessage(None)
    StartMessageTimer(ClientOnlyMessageIndex)
EndEvent
