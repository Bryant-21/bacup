; Candy economy of the Watoga High S.M.A.R.T. Choice Machine (TERM 51AA00 and its
; submenus 51AA03/51AA04/51AA05/52EC31). Deposited Halloween candy becomes credit in
; CB02_CandyStoredInVendingMachine on the player; the submenu fragments spend it
; through Purchase. The confirmation page picks its text from the two confirmation
; values. FO4 only replaces tokens with form names, so the conversion splits the
; StoredCandy/PlayerCandy count tokens into digit tokens filled from KeypadDigitsList.
Event OnMenuItemRun(Int auiMenuItemID, ObjectReference akTerminalRef)
    RefreshCandyTokens(akTerminalRef)
EndEvent

Bool Function Purchase(ObjectReference akTerminalRef, Form akItem, Int aiPrice, Bool abNotKidFriendly)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || akItem == None
        Return False
    EndIf
    playerRef.SetValue(CB02_VendingMachineNotKidFriendlyConfirmation, 0.0)
    If playerRef.GetValue(CB02_CandyStoredInVendingMachine) < aiPrice as Float
        playerRef.SetValue(CB02_VendingMachineConfirmation, 0.0)
        RefreshCandyTokens(akTerminalRef)
        Return False
    EndIf
    playerRef.ModValue(CB02_CandyStoredInVendingMachine, -aiPrice as Float)
    playerRef.SetValue(CB02_VendingMachineConfirmation, 1.0)
    If abNotKidFriendly
        playerRef.SetValue(CB02_VendingMachineNotKidFriendlyConfirmation, 1.0)
    EndIf
    playerRef.AddItem(akItem, 1, False)
    RefreshCandyTokens(akTerminalRef)
    Return True
EndFunction

Function DepositCandy(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If playerRef == None || CB02_HolloweenCandy == None
        Return
    EndIf
    Int candy = playerRef.GetItemCount(CB02_HolloweenCandy)
    If candy > 0
        playerRef.RemoveItem(CB02_HolloweenCandy, candy, True, akTerminalRef)
        playerRef.ModValue(CB02_CandyStoredInVendingMachine, candy as Float)
    EndIf
    RefreshCandyTokens(akTerminalRef)
EndFunction

Function RefreshCandyTokens(ObjectReference akTerminalRef)
    Actor playerRef = Game.GetPlayer()
    If akTerminalRef == None || playerRef == None
        Return
    EndIf
    FormList digits = Game.GetFormFromFile(0x0038EF5B, "SeventySix.esm") as FormList
    If digits == None
        Return
    EndIf
    SetDigitTokens(akTerminalRef, digits, "StoredCandy", playerRef.GetValue(CB02_CandyStoredInVendingMachine) as Int)
    Int carried = 0
    If CB02_HolloweenCandy != None
        carried = playerRef.GetItemCount(CB02_HolloweenCandy)
    EndIf
    SetDigitTokens(akTerminalRef, digits, "PlayerCandy", carried)
EndFunction

; KeypadDigitsList holds Generic_Number00-09_Msg at 0-9 and Generic_Blank_Msg at 10.
Function SetDigitTokens(ObjectReference akTerminalRef, FormList akDigits, String asLabel, Int aiValue)
    Int value = aiValue
    If value < 0
        value = 0
    ElseIf value > 99999
        value = 99999
    EndIf
    Int position = 5
    Int remaining = value
    While position >= 1
        Form digitForm
        If remaining > 0 || position == 5
            digitForm = akDigits.GetAt(remaining % 10)
            remaining = remaining / 10
        Else
            digitForm = akDigits.GetAt(10)
        EndIf
        If digitForm != None
            akTerminalRef.AddTextReplacementData(asLabel + position, digitForm)
        EndIf
        position -= 1
    EndWhile
EndFunction
