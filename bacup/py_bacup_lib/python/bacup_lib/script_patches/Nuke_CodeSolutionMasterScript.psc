; FO76 Debug.TraceLog is unavailable in FO4. Preserve the Bool contract with
; FO4's user-log API.

Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity, String DejaSubChannel, Bool bShowNormalTrace)
    Debug.OpenUserLog("Nukes")
    Return Debug.TraceUser("Nukes", CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction

Event OnLoad()
    Parent.OnLoad()
    UpdateLocalKeyword()
EndEvent

Function UpdateLocalKeyword()
    If SiloGroupID < 0 || SiloGroupID > 2 || !Is3DLoaded()
        Return
    EndIf
    Quest masterQuest = Game.GetFormFromFile(0x003CD064, "SeventySix.esm") as Quest
    Nuke_MasterScript master = masterQuest as Nuke_MasterScript
    If master != None && B21:KeypadNative.Ready()
        DisplayMessage(master.LocalKeywordLetters(SiloGroupID))
    EndIf
    StartTimer(10.0, 76011)
EndFunction

Event OnTimer(Int aiTimerID)
    If aiTimerID == 76011
        UpdateLocalKeyword()
    Else
        Parent.OnTimer(aiTimerID)
    EndIf
EndEvent

Event OnUnload()
    CancelTimer(76011)
EndEvent
