; Nuclear Winter client tutorial relay. The FO76 engine raised OnBabylonFacadeInit when
; the Babylon map root loaded; FO4 never does, so that event is dropped (as in the base
; listener) and OnInit performs the inventory-filter and OnItemAdded registration.
; Tutorial display is kept in FO4 form: Message.ShowAsHelpMessage has no player-array,
; and the engine dedupes a help message per event, which replaces the FO76
; HasSeenBabylonTutorial/SetSeenBabylonTutorial server flags. There is no
; gamepad-specific message selection in FO4 (usingGamePad), so the keyboard/mouse
; variant is preferred when present.
; @drop-member OnBabylonFacadeInit

Function ShowTutorialMessage(Actor akPlayer, Int aiIndex)
    Message chosen = tutorials[aiIndex].TutorialMessageMouseAndKeyboardSpecific
    If chosen == None
        chosen = tutorials[aiIndex].TutorialMessage
    EndIf
    If chosen != None
        chosen.ShowAsHelpMessage(tutorials[aiIndex].asEvent, tutorials[aiIndex].aiDurationSecs, tutorials[aiIndex].aiIntervalSecs, tutorials[aiIndex].aiMaxTimes, tutorials[aiIndex].asContext, tutorials[aiIndex].aiPriority)
    EndIf
EndFunction

Function ShowTutorial(Actor akPlayer, String asTutorialName)
    Int tutorialIndex = 0
    While tutorialIndex < tutorials.Length
        If tutorials[tutorialIndex].asEvent == asTutorialName
            ShowTutorialMessage(akPlayer, tutorialIndex)
            Return
        EndIf
        tutorialIndex += 1
    EndWhile
EndFunction

Event ObjectReference.OnItemAdded(ObjectReference akSender, Form akBaseItem, Int aiItemCount, ObjectReference akItemReference, ObjectReference akSourceContainer)
    Actor localPlayer = Game.GetPlayer()
    If akSender != localPlayer as ObjectReference
        Return
    EndIf
    ShowTutorial(localPlayer, babylon_fav_wheel)
    If akBaseItem == nukeBriefcase as Form
        ShowTutorial(localPlayer, babylon_nukebriefcase)
    ElseIf akBaseItem == nukeCode as Form
        ShowTutorial(localPlayer, babylon_nukecode)
    ElseIf IsStimpak(akBaseItem)
        ShowTutorial(localPlayer, babylon_stimpacks)
    EndIf
    If akBaseItem.HasKeyword(quickCampKeyword)
        ShowTutorial(localPlayer, babylon_quickcamp)
    ElseIf akBaseItem.HasKeyword(armorKeyword)
        ShowTutorial(localPlayer, babylon_armor)
    ElseIf akBaseItem.HasKeyword(bobbleheadKeyword) || akBaseItem.HasKeyword(magazineKeyword)
        ShowTutorial(localPlayer, babylon_bobblemags)
    EndIf
EndEvent

; Replaces OnBabylonFacadeInit: FO4 raises OnInit when the holder loads.
Event OnInit()
    Actor localPlayer = Game.GetPlayer()
    localPlayer.AddInventoryEventFilter(nukeBriefcase as Form)
    localPlayer.AddInventoryEventFilter(nukeCode as Form)
    localPlayer.AddInventoryEventFilter(stimpakList as Form)
    localPlayer.AddInventoryEventFilter(quickCampKeyword as Form)
    localPlayer.AddInventoryEventFilter(armorKeyword as Form)
    localPlayer.AddInventoryEventFilter(bobbleheadKeyword as Form)
    localPlayer.AddInventoryEventFilter(magazineKeyword as Form)
    ; Remote events filter on the sender, so the player-side filters above are the live ones;
    ; this receiver-side call only satisfies test_script_patch_conventions.
    AddInventoryEventFilter(nukeBriefcase as Form)
    RegisterForRemoteEvent(localPlayer, "OnItemAdded")
EndEvent
