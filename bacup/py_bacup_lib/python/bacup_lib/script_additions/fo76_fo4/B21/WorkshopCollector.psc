Scriptname B21:WorkshopCollector Extends ObjectReference

LeveledItem Property Produce Auto Const
GlobalVariable Property IntervalHours Auto Const
Int Property MaxStoredItems = 50 Auto Const

Int ProductionTimerID = 1

Float fLastProduced = -1.0
; Power is only observable while the reference is loaded, so time spent unloaded
; is credited according to the state it was last seen in.
Bool bPowered = false

Event OnInit()
    fLastProduced = Utility.GetCurrentGameTime()
EndEvent

Event OnLoad()
    Accrue()
    bPowered = IsPowered()
    StartProductionTimer()
EndEvent

Event OnPowerOn(ObjectReference akPowerGenerator)
    Accrue()
    bPowered = true
EndEvent

Event OnPowerOff()
    Accrue()
    bPowered = false
EndEvent

Event OnUnload()
    CancelTimerGameTime(ProductionTimerID)
EndEvent

Event OnTimerGameTime(int aiTimerID)
    If aiTimerID != ProductionTimerID
        Return
    EndIf
    Accrue()
    If Is3DLoaded()
        bPowered = IsPowered()
        StartProductionTimer()
    EndIf
EndEvent

Event OnActivate(ObjectReference akActionRef)
    Accrue()
    bPowered = IsPowered()
EndEvent

Function StartProductionTimer()
    If IntervalHours == None
        Return
    EndIf
    Float hours = IntervalHours.GetValue()
    If hours > 0.0
        StartTimerGameTime(hours, ProductionTimerID)
    EndIf
EndFunction

; Extractors need power; beehives and other passive collectors carry no power keyword.
Bool Function NeedsPower()
    Keyword canBePowered = Game.GetFormFromFile(0x0003037E, "Fallout4.esm") as Keyword
    Return canBePowered && HasKeyword(canBePowered)
EndFunction

Function Accrue()
    If Produce == None || IntervalHours == None || MaxStoredItems <= 0
        Return
    EndIf

    Float intervalDays = IntervalHours.GetValue() / 24.0
    If intervalDays <= 0.0
        Return
    EndIf

    Float now = Utility.GetCurrentGameTime()
    ; Unseeded (script newly attached to an existing reference) or a clock that
    ; moved backwards would otherwise bank an unbounded backlog in one step.
    If fLastProduced < 0.0 || fLastProduced > now
        fLastProduced = now
        Return
    EndIf

    Int elapsedIntervals = ((now - fLastProduced) / intervalDays) as Int
    If elapsedIntervals <= 0
        Return
    EndIf
    ; Consume every whole interval even when the container is full, so time spent
    ; at capacity does not accrue a burst that lands the moment it is emptied.
    fLastProduced += (elapsedIntervals as Float) * intervalDays
    If !bPowered && NeedsPower()
        Return
    EndIf

    ; Capacity comes from the live contents: FO4 only delivers inventory events
    ; through an inventory filter, so a removal-tracked counter never drains.
    While elapsedIntervals > 0 && GetItemCount(None) < MaxStoredItems
        AddItem(Produce, 1, true)
        elapsedIntervals -= 1
    EndWhile
EndFunction
