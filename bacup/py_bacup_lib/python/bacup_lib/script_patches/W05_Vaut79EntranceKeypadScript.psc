; The Vault 79 entrance keypad (56BA45). A correct code is what grants
; W05_PlayerCanAccessVault79Entrance, the gate on the LinkCustom01 laser grid (5673CA) in front of
; the mech door. The keypad's own DefaultKeypadScript opens the door (its default link).

Event OnInit()
    RegisterForKeypadSuccess()
EndEvent

Event OnLoad()
    RegisterForKeypadSuccess()
EndEvent

Function RegisterForKeypadSuccess()
    DefaultKeypadScript keypad = (Self as ObjectReference) as DefaultKeypadScript
    If keypad != None
        RegisterForCustomEvent(keypad, "KeypadSuccess")
    EndIf
EndFunction

Event DefaultKeypadScript.KeypadSuccess(DefaultKeypadScript akSender, Var[] akArgs)
    Actor playerRef = Game.GetPlayer()
    If akArgs == None || akArgs.Length < 2 || (akArgs[1] as ObjectReference) != playerRef
        Return
    EndIf
    If W05_PlayerCanAccessVault79Entrance != None
        playerRef.SetValue(W05_PlayerCanAccessVault79Entrance, 1.0)
    EndIf

    ; The converted VMAD lost this property's keyword; the placed link itself uses Fallout4.esm's LinkCustom01.
    Keyword gridLink = LinkCustom01
    If gridLink == None
        gridLink = Game.GetFormFromFile(0x0005D5E6, "Fallout4.esm") as Keyword
    EndIf
    LaserGridScript entranceGrid = GetLinkedRef(gridLink) as LaserGridScript
    If entranceGrid != None
        entranceGrid.RequestReevaluateConditions()
    EndIf
EndEvent
