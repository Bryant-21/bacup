; FO4 port of the FO76 tutorial tracker. Dropped, with no FO4 trigger: the custom
; client events (baseline, first camera target, camera scope, perk added, opt-in quest
; event), the nuclear-winter-mode early-out, and multi-player message targeting.
; Seen/unseen is kept in-script: TutorialSeenInt is stored as -1 - value once shown,
; because FO76's server-side HasSeenTutorial/SetSeenTutorial flags do not exist.
; Location filters do not exist either; OnLocationChange is unfiltered and the
; seen flag stops repeats.
; @drop-member OnPlayerHasTutorialBaseline
; @drop-member OnFirstCameraTargetDetected
; @drop-member OnEnterCameraScopeMenu
; @drop-member OnPerkAdded
; @drop-member OnPlayerCanOptIntoQuestEvent

Event OnInit()
    GoToState("LocalPlayer")
EndEvent

Int Function FindTutorial(String asEventName)
    If TutorialMessageData == None
        Return -1
    EndIf
    Return TutorialMessageData.FindStruct("asEvent", asEventName)
EndFunction

Bool Function IsTutorialSeen(Int aiIndex)
    Return TutorialMessageData[aiIndex].TutorialSeenInt < 0
EndFunction

Function MarkTutorialSeen(Int aiIndex)
    TutorialMessageDatum datum = TutorialMessageData[aiIndex]
    If datum.TutorialSeenInt >= 0
        datum.TutorialSeenInt = -1 - datum.TutorialSeenInt
    EndIf
EndFunction

Function ShowHelpMessageClientAndFlagTutorialSeen(String EventName)
    Int iFound = FindTutorial(EventName)
    If iFound < 0 || IsTutorialSeen(iFound)
        Return
    EndIf

    TutorialMessageDatum datum = TutorialMessageData[iFound]
    If datum.DelayTime > 0.0 && !datum.DelayFinished
        StartTimer(datum.DelayTime, iFound)
        Return
    EndIf

    String nextEvent = datum.asEventNext
    DoShowMessageOnClient(iFound)
    MarkTutorialSeen(iFound)
    datum.DelayFinished = False
    If nextEvent != ""
        ShowHelpMessageClientAndFlagTutorialSeen(nextEvent)
    EndIf
EndFunction

Function CheckForFilterOnClient(String AsEventString, Form FilterForm, Int FilterType)
    Int iFound = FindTutorial(AsEventString)
    If iFound < 0 || IsTutorialSeen(iFound)
        Return
    EndIf

    If FilterType == iFilterType_Inventory
        AddInventoryEventFilter(FilterForm)
    ElseIf FilterType == iFilterType_RadiationDamage
        RegisterForRadiationDamageEvent(Self as ScriptObject)
    ElseIf FilterType == iFilterType_TutorialEvent
        RegisterForTutorialEvent(AsEventString)
    ElseIf FilterType == iFilterType_MenuOpenClose
        RegisterForMenuOpenCloseEvent(AsEventString)
    EndIf
EndFunction

Function DoShowMessageOnClient(Int iFound)
    TutorialMessageDatum datum = TutorialMessageData[iFound]
    Message helpMessage = datum.TutorialMessage
    If datum.TutorialMessageMouseAndKeyboardSpecific != None && !Game.UsingGamepad()
        helpMessage = datum.TutorialMessageMouseAndKeyboardSpecific
    EndIf
    If helpMessage != None
        helpMessage.ShowAsHelpMessage(datum.asEvent, datum.aiDurationSecs as Float, datum.aiIntervalSecs as Float, datum.aiMaxTimes, datum.asContext, datum.aiPriority)
    EndIf
EndFunction

State LocalPlayer
    Event OnItemEquipped(Form akBaseObject, ObjectReference akReference)
        Weapon objectAsWeapon = akBaseObject as Weapon
        If objectAsWeapon != None && objectAsWeapon.GetAmmo() != None
            ShowHelpMessageClientAndFlagTutorialSeen("AimAndAttack")
        ElseIf akBaseObject.HasKeyword(WeaponTypeGrenade)
            ShowHelpMessageClientAndFlagTutorialSeen("Grenade")
        ElseIf objectAsWeapon != None
            ShowHelpMessageClientAndFlagTutorialSeen("BlockAndAttack")
        EndIf
    EndEvent

    Event OnMenuOpenCloseEvent(String asMenuName, Bool abOpening)
        String TutorialString = ""
        If asMenuName == "LockpickingMenu"
            If abOpening
                TutorialString = "Lockpicking"
                UnregisterForMenuOpenCloseEvent("LockpickingMenu")
            EndIf
        ElseIf asMenuName == "TerminalMenu"
            TutorialString = "TerminalMenu"
            UnregisterForMenuOpenCloseEvent("TerminalMenu")
        ElseIf asMenuName == "VATSMenu"
            Int vatsTutorial = FindTutorial("VATSMenu")
            If vatsTutorial >= 0 && !IsTutorialSeen(vatsTutorial)
                TutorialString = "VATSMenu"
                UnregisterForMenuOpenCloseEvent("VATSMenu")
            EndIf
        ElseIf asMenuName == "MapMenu"
            Int fastTravelEventTutorial = FindTutorial("FastTravelEvents")
            If TutorialEventActive.GetValueInt() >= 1 && fastTravelEventTutorial >= 0 && IsTutorialSeen(fastTravelEventTutorial)
                TutorialString = "MapEvents"
                UnregisterForMenuOpenCloseEvent("MapMenu")
            EndIf
        EndIf
        If TutorialString != ""
            ShowHelpMessageClientAndFlagTutorialSeen(TutorialString)
        EndIf
    EndEvent

    Event OnLocationChange(Location akOldLoc, Location akNewLoc)
        If IsInLocation(RegionMTRLocation)
            ShowHelpMessageClientAndFlagTutorialSeen("DiseaseRiskLocation_MTR")
        ElseIf IsInLocation(RegionSwampForestLocation)
            ShowHelpMessageClientAndFlagTutorialSeen("DiseaseRiskLocation_Mire")
        EndIf
    EndEvent
EndState

Bool Function Trace(ScriptObject CallingObject, String asTextToPrint, Int aiSeverity, String DejaSubChannel, Bool bShowNormalTrace) Global
    Debug.OpenUserLog("PlayerTutorial")
    Return Debug.TraceUser("PlayerTutorial", CallingObject as String + ": " + asTextToPrint, aiSeverity)
EndFunction
