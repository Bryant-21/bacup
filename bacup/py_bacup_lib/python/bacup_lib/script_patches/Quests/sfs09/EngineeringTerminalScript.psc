Event OnAliasInit()
    If ShutdownCodeTerminal != None
        RegisterForRemoteEvent(ShutdownCodeTerminal, "OnMenuItemRun")
    EndIf
EndEvent

Event OnAliasShutdown()
    If ShutdownCodeTerminal != None
        UnregisterForRemoteEvent(ShutdownCodeTerminal, "OnMenuItemRun")
    EndIf
EndEvent

; The engineering terminal hands out ARIC-4's shutdown passcode and points the player at the mainframe.
Event Terminal.OnMenuItemRun(Terminal akSender, Int auiMenuItemID, ObjectReference akTerminalRef)
    If auiMenuItemID != ShutdownCodeMenuItemID || akTerminalRef != GetReference()
        Return
    EndIf
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || ShutdownCode == None || SFS09_Habitat == None || !SFS09_Habitat.IsRunning()
        Return
    EndIf
    If SFS09_Habitat.IsStageDone(170) || SFS09_Habitat.IsStageDone(200) || playerRef.GetItemCount(ShutdownCode) > 0
        Return
    EndIf
    ObjectReference codeRef = playerRef.PlaceAtMe(ShutdownCode, 1, False, False, False)
    If codeRef == None
        Return
    EndIf
    If ShutdownCodeAlias != None
        ShutdownCodeAlias.AddRef(codeRef)
    EndIf
    playerRef.AddItem(codeRef, 1, False)
    If SFS09_Habitat_Misc_StartKeyword != None && SFS09_Habitat_Misc != None && !SFS09_Habitat_Misc.IsRunning()
        SFS09_Habitat_Misc_StartKeyword.SendStoryEvent(None, playerRef)
    EndIf
EndEvent
