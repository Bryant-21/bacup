; Holds the one-time key a GenerateKeypadCombination terminal commits to it and
; unlocks its linked door when that key is entered (BMO bunker key 02).
Function SetCode(Int aiCode, Int aiDigits)
    B21Code = aiCode
    B21Digits = aiDigits
EndFunction

Event OnActivate(ObjectReference akActionRef)
    If akActionRef == None || akActionRef != Game.GetPlayer() || B21Reading
        Return
    EndIf
    ObjectReference doorRef = GetLinkedRef()
    If doorRef == None || (!doorRef.IsLocked() && doorRef.GetOpenState() == 1)
        Return
    EndIf
    If B21Digits <= 0 || B21Code <= 0
        Debug.Notification("The keypad needs a one-time key.")
        Return
    EndIf
    If !B21:KeypadNative.Ready()
        Debug.Notification("The Tales keypad runtime is unavailable.")
        Return
    EndIf
    B21Reading = True
    Int expectedCode = B21Code
    Int enteredCode = B21:KeypadNative.ReadCode(Self, B21Digits)
    B21Reading = False
    If enteredCode < 0
        Return
    EndIf
    If enteredCode != expectedCode || expectedCode != B21Code
        Debug.Notification("The keypad rejects that code.")
        Return
    EndIf
    doorRef.Unlock()
    doorRef.SetOpen(True)
    Debug.Notification("The keypad accepts the code.")
EndEvent

Event OnLoad()
    B21Reading = False
EndEvent
